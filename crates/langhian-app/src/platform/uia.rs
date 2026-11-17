//! Windows UI Automation helper for browser, Next.js/React, and native text input inspection.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableKind {
    Editable,
    Password,
    NonEditable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiaElementSnapshot {
    pub control_type_id: i32,
    pub aria_role: String,
    pub aria_properties: String,
    pub is_password: bool,
    pub has_value_pattern: bool,
    pub value_is_read_only: Option<bool>,
    pub has_text_pattern: bool,
    pub has_text_edit_pattern: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FocusedInputInfo {
    pub editable: Option<EditableKind>,
    pub selected_text: Option<String>,
    pub value_text: Option<String>,
    pub anchor_rect: Option<(i32, i32, i32)>,
}

/// Pure classifier for a UIA element snapshot so native, Chromium/Edge, and Next.js ARIA
/// custom inputs (`contenteditable`, `role="textbox"`, `searchbox`, `combobox`) are recognized
/// uniformly while password controls and non-text UI widgets are filtered safely.
pub fn classify_element(snap: &UiaElementSnapshot) -> EditableKind {
    if snap.is_password {
        return EditableKind::Password;
    }

    let role_lower = snap.aria_role.trim().to_ascii_lowercase();
    let has_editable_aria = role_lower
        .split_whitespace()
        .any(|r| matches!(r, "textbox" | "searchbox" | "combobox" | "spinbutton"));

    let props_lower = snap.aria_properties.to_ascii_lowercase();
    let aria_readonly_true = props_lower.contains("readonly=true");
    let aria_readonly_false = props_lower.contains("readonly=false");
    let aria_multiline = props_lower.contains("multiline=true");

    if snap.has_text_edit_pattern || snap.value_is_read_only == Some(false) || aria_readonly_false {
        return EditableKind::Editable;
    }

    // UIA_EditControlTypeId = 50004, UIA_ComboBoxControlTypeId = 50003
    if (snap.control_type_id == 50004
        || snap.control_type_id == 50003
        || has_editable_aria
        || aria_multiline)
        && !aria_readonly_true
        && snap.value_is_read_only != Some(true)
    {
        return EditableKind::Editable;
    }

    // Explicit non-text static chrome/menu widgets (never block Document/Pane/Group/Custom in case
    // Chromium hasn't finished exposing a custom editor node or canvas-backed input).
    if matches!(
        snap.control_type_id,
        50000 // Button
            | 50002 // CheckBox
            | 50005 // Hyperlink
            | 50006 // Image
            | 50009 // Menu
            | 50010 // MenuBar
            | 50011 // MenuItem
            | 50013 // RadioButton
            | 50014 // ScrollBar
            | 50015 // Slider
            | 50019 // TabItem
            | 50021 // ToolBar
            | 50024 // TreeItem
    ) && !has_editable_aria
    {
        return EditableKind::NonEditable;
    }

    EditableKind::Unknown
}

pub fn probe_focused_input(foreground_hwnd: isize, wake_browser: bool) -> Option<FocusedInputInfo> {
    #[cfg(target_os = "windows")]
    {
        win_uia::probe_focused_input_impl(foreground_hwnd, wake_browser)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (foreground_hwnd, wake_browser);
        None
    }
}

pub fn focused_anchor_rect(foreground_hwnd: isize, wake_browser: bool) -> Option<(i32, i32, i32)> {
    probe_focused_input(foreground_hwnd, wake_browser).and_then(|info| info.anchor_rect)
}

pub fn try_select_all_text(foreground_hwnd: isize) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        win_uia::try_select_all_text_impl(foreground_hwnd)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = foreground_hwnd;
        None
    }
}

#[cfg(target_os = "windows")]
mod win_uia {
    use super::{EditableKind, FocusedInputInfo, UiaElementSnapshot, classify_element};
    use std::ffi::c_void;
    use std::thread;
    use std::time::Duration;
    use windows::Win32::Foundation::{HWND, RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
        SafeArrayUnaccessData,
    };
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, CUIAutomation, IUIAutomation, IUIAutomationElement,
        IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextEditPatternId,
        UIA_TextPatternId, UIA_ValuePatternId,
    };
    use windows::core::{IUnknown, Interface};

    const OBJID_CLIENT_U32: u32 = (-4i32) as u32;

    struct ComGuard(bool);

    impl ComGuard {
        fn init() -> Option<Self> {
            let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            if hr == S_OK || hr == S_FALSE {
                Some(Self(true))
            } else if hr == RPC_E_CHANGED_MODE {
                // Thread already initialized COM (e.g., STA on main UI thread); keep without uninit.
                Some(Self(false))
            } else {
                None
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    fn create_automation() -> Option<IUIAutomation> {
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL).ok() }
    }

    fn wake_chromium_accessibility(foreground_hwnd: isize) {
        if foreground_hwnd == 0 {
            return;
        }
        unsafe {
            let hwnd = HWND(foreground_hwnd as *mut c_void);
            let mut raw: *mut c_void = std::ptr::null_mut();
            if AccessibleObjectFromWindow(hwnd, OBJID_CLIENT_U32, &IUnknown::IID, &mut raw).is_ok()
                && !raw.is_null()
            {
                drop(IUnknown::from_raw(raw));
            }
        }
    }

    fn is_likely_uninitialized_container(el: &IUIAutomationElement) -> bool {
        let ct = unsafe { el.CurrentControlType() }.map(|c| c.0).unwrap_or(0);
        // Window (50032), Pane (50033), or root Document (50030) without TextEdit/Edit role
        if !matches!(ct, 50030 | 50032 | 50033) {
            return false;
        }
        let has_value_edit =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
                .ok()
                .and_then(|vp| unsafe { vp.CurrentIsReadOnly() }.ok())
                .is_some_and(|ro| !ro.as_bool());
        let has_text_edit = unsafe { el.GetCurrentPattern(UIA_TextEditPatternId) }.is_ok();
        !(has_value_edit || has_text_edit)
    }

    fn acquire_focused_element(
        automation: &IUIAutomation,
        foreground_hwnd: isize,
        wake_browser: bool,
    ) -> Option<IUIAutomationElement> {
        let initial = unsafe { automation.GetFocusedElement() }.ok();
        if !wake_browser {
            return initial;
        }

        let needs_wake = match &initial {
            None => foreground_hwnd != 0,
            Some(el) => is_likely_uninitialized_container(el),
        };
        if !needs_wake {
            return initial;
        }

        wake_chromium_accessibility(foreground_hwnd);
        for _ in 0..2 {
            thread::sleep(Duration::from_millis(22));
            if let Ok(el) = unsafe { automation.GetFocusedElement() }
                && !is_likely_uninitialized_container(&el)
            {
                return Some(el);
            }
        }
        initial.or_else(|| unsafe { automation.GetFocusedElement() }.ok())
    }

    fn extract_text_range_anchor(
        range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
    ) -> Option<(i32, i32, i32)> {
        let psa = unsafe { range.GetBoundingRectangles() }.ok()?;
        if psa.is_null() {
            return None;
        }
        unsafe {
            let lbound = SafeArrayGetLBound(psa, 1).ok();
            let ubound = SafeArrayGetUBound(psa, 1).ok();
            let mut out = None;
            if let (Some(lb), Some(ub)) = (lbound, ubound) {
                let count = ub.saturating_sub(lb).saturating_add(1);
                if count >= 4 {
                    let mut ptr: *mut c_void = std::ptr::null_mut();
                    if SafeArrayAccessData(psa, &mut ptr).is_ok() && !ptr.is_null() {
                        let vals = std::slice::from_raw_parts(ptr as *const f64, count as usize);
                        let left = vals[0];
                        let top = vals[1];
                        let width = vals[2];
                        let height = vals[3];
                        if width >= 0.0 && height > 0.0 {
                            out = Some((
                                left.round() as i32,
                                top.round() as i32,
                                (top + height).round() as i32,
                            ));
                        }
                        let _ = SafeArrayUnaccessData(psa);
                    }
                }
            }
            let _ = SafeArrayDestroy(psa);
            out
        }
    }

    pub fn probe_focused_input_impl(
        foreground_hwnd: isize,
        wake_browser: bool,
    ) -> Option<FocusedInputInfo> {
        let _com = ComGuard::init()?;
        let automation = create_automation()?;
        let el = acquire_focused_element(&automation, foreground_hwnd, wake_browser)?;

        let control_type_id = unsafe { el.CurrentControlType() }.map(|c| c.0).unwrap_or(0);
        let aria_role = unsafe { el.CurrentAriaRole() }
            .map(|s| s.to_string())
            .unwrap_or_default();
        let aria_properties = unsafe { el.CurrentAriaProperties() }
            .map(|s| s.to_string())
            .unwrap_or_default();
        let is_password = unsafe { el.CurrentIsPassword() }
            .map(|b| b.as_bool())
            .unwrap_or(false);

        let value_pat =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }.ok();
        let value_is_read_only = value_pat
            .as_ref()
            .and_then(|vp| unsafe { vp.CurrentIsReadOnly() }.ok())
            .map(|b| b.as_bool());

        let text_pat =
            unsafe { el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.ok();
        let has_text_edit_pattern = unsafe { el.GetCurrentPattern(UIA_TextEditPatternId) }.is_ok();

        let snap = UiaElementSnapshot {
            control_type_id,
            aria_role,
            aria_properties,
            is_password,
            has_value_pattern: value_pat.is_some(),
            value_is_read_only,
            has_text_pattern: text_pat.is_some(),
            has_text_edit_pattern,
        };
        let editable = classify_element(&snap);
        if editable == EditableKind::Password {
            return Some(FocusedInputInfo {
                editable: Some(EditableKind::Password),
                selected_text: None,
                value_text: None,
                anchor_rect: None,
            });
        }

        let mut selected_text = None;
        let mut anchor_rect = None;

        if let Some(ref tp) = text_pat
            && let Ok(sel_arr) = unsafe { tp.GetSelection() }
        {
            let len = unsafe { sel_arr.Length() }.unwrap_or(0);
            let mut parts = Vec::new();
            for i in 0..len.min(8) {
                if let Ok(range) = unsafe { sel_arr.GetElement(i) } {
                    if anchor_rect.is_none() {
                        anchor_rect = extract_text_range_anchor(&range);
                    }
                    if let Ok(txt) = unsafe { range.GetText(32_000) } {
                        let s = txt.to_string();
                        if !s.trim().is_empty() {
                            parts.push(s);
                        }
                    }
                }
            }
            if !parts.is_empty() {
                selected_text = Some(parts.join("\n"));
            }
        }

        if anchor_rect.is_none()
            && editable == EditableKind::Editable
            && let Ok(rc) = unsafe { el.CurrentBoundingRectangle() }
            && rc.right > rc.left
            && rc.bottom > rc.top
        {
            anchor_rect = Some((rc.left, rc.top, rc.bottom));
        }

        let value_text = if editable == EditableKind::Editable {
            value_pat
                .as_ref()
                .and_then(|vp| unsafe { vp.CurrentValue() }.ok())
                .map(|b| b.to_string())
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    text_pat
                        .as_ref()
                        .and_then(|tp| unsafe { tp.DocumentRange() }.ok())
                        .and_then(|range| unsafe { range.GetText(32_000) }.ok())
                        .map(|b| b.to_string())
                        .filter(|s| !s.trim().is_empty())
                })
        } else {
            None
        };

        Some(FocusedInputInfo {
            editable: Some(editable),
            selected_text,
            value_text,
            anchor_rect,
        })
    }

    pub fn try_select_all_text_impl(foreground_hwnd: isize) -> Option<String> {
        let _com = ComGuard::init()?;
        let automation = create_automation()?;
        let el = acquire_focused_element(&automation, foreground_hwnd, false)?;
        let is_password = unsafe { el.CurrentIsPassword() }
            .map(|b| b.as_bool())
            .unwrap_or(false);
        if is_password {
            return None;
        }
        let tp = unsafe { el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
            .ok()?;
        let range = unsafe { tp.DocumentRange() }.ok()?;
        let _ = unsafe { range.Select() };
        let txt = unsafe { range.GetText(32_000) }.ok()?.to_string();
        if txt.trim().is_empty() {
            None
        } else {
            Some(txt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_nextjs_aria_contenteditable_and_password_fields_accurately() {
        // React / Next.js combobox or searchbox without native <input>
        let github_search = UiaElementSnapshot {
            control_type_id: 50026, // Group / custom web node
            aria_role: "combobox".to_string(),
            aria_properties: String::new(),
            is_password: false,
            has_value_pattern: true,
            value_is_read_only: None,
            has_text_pattern: true,
            has_text_edit_pattern: false,
        };
        assert_eq!(classify_element(&github_search), EditableKind::Editable);

        // Next.js custom <div contenteditable="true" role="textbox">
        let contenteditable = UiaElementSnapshot {
            control_type_id: 50030, // Document
            aria_role: "textbox".to_string(),
            aria_properties: "multiline=true".to_string(),
            is_password: false,
            has_value_pattern: false,
            value_is_read_only: Some(false),
            has_text_pattern: true,
            has_text_edit_pattern: true,
        };
        assert_eq!(classify_element(&contenteditable), EditableKind::Editable);

        // Password fields must always be blocked
        let pwd = UiaElementSnapshot {
            control_type_id: 50004,
            aria_role: "textbox".to_string(),
            aria_properties: String::new(),
            is_password: true,
            has_value_pattern: true,
            value_is_read_only: Some(false),
            has_text_pattern: false,
            has_text_edit_pattern: false,
        };
        assert_eq!(classify_element(&pwd), EditableKind::Password);

        // Static UI button should not trigger blind Ctrl+A
        let button = UiaElementSnapshot {
            control_type_id: 50000,
            aria_role: "button".to_string(),
            aria_properties: String::new(),
            is_password: false,
            has_value_pattern: false,
            value_is_read_only: None,
            has_text_pattern: false,
            has_text_edit_pattern: false,
        };
        assert_eq!(classify_element(&button), EditableKind::NonEditable);
    }
}
