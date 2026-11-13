use crate::platform::capture::{CapturedScreen, NormalizedRect};
use iced::keyboard::key::Named;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Program, Stroke};
use iced::widget::{container, image, stack, text};
use iced::{
    Background, Border, Color, ContentFit, Element, Length, Point, Rectangle, Renderer, Size,
    Theme, keyboard, mouse,
};

#[derive(Debug, Clone)]
pub struct SnipState {
    pub screen: CapturedScreen,
    pub preview_handle: image::Handle,
    pub drag_start: Option<Point>,
    pub drag_current: Option<Point>,
}

impl SnipState {
    pub fn new(screen: CapturedScreen) -> Self {
        let preview_handle = image::Handle::from_rgba(
            screen.image_width,
            screen.image_height,
            screen.rgba_bytes.clone(),
        );
        Self {
            screen,
            preview_handle,
            drag_start: None,
            drag_current: None,
        }
    }

    pub fn selection_rect(&self) -> Option<NormalizedRect> {
        let (a, b) = (self.drag_start?, self.drag_current?);
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        let width = (a.x - b.x).abs();
        let height = (a.y - b.y).abs();
        if width < 6.0 || height < 6.0 {
            None
        } else {
            Some(NormalizedRect {
                x,
                y,
                width,
                height,
            })
        }
    }
}

#[derive(Debug, Clone)]
pub enum SnipEvent {
    DragStart(Point),
    DragMove(Point),
    DragEnd(Size),
    Cancel,
}

struct SnipCanvas {
    selection: Option<NormalizedRect>,
}

impl Program<SnipEvent> for SnipCanvas {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<SnipEvent>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => cursor
                .position_in(bounds)
                .map(|p| Action::publish(SnipEvent::DragStart(p)).and_capture()),
            Event::Mouse(mouse::Event::CursorMoved { .. }) => cursor
                .position_in(bounds)
                .map(|p| Action::publish(SnipEvent::DragMove(p))),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Action::publish(SnipEvent::DragEnd(bounds.size())).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                Some(Action::publish(SnipEvent::Cancel).and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(Named::Escape),
                ..
            }) => Some(Action::publish(SnipEvent::Cancel).and_capture()),
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let mask_color = Color::from_rgba8(8, 11, 18, 0.54);

        if let Some(sel) = self.selection {
            // Top dark band
            if sel.y > 0.0 {
                frame.fill_rectangle(Point::ORIGIN, Size::new(bounds.width, sel.y), mask_color);
            }
            // Bottom dark band
            let bottom_y = sel.y + sel.height;
            if bottom_y < bounds.height {
                frame.fill_rectangle(
                    Point::new(0.0, bottom_y),
                    Size::new(bounds.width, bounds.height - bottom_y),
                    mask_color,
                );
            }
            // Left dark band
            if sel.x > 0.0 {
                frame.fill_rectangle(
                    Point::new(0.0, sel.y),
                    Size::new(sel.x, sel.height),
                    mask_color,
                );
            }
            // Right dark band
            let right_x = sel.x + sel.width;
            if right_x < bounds.width {
                frame.fill_rectangle(
                    Point::new(right_x, sel.y),
                    Size::new(bounds.width - right_x, sel.height),
                    mask_color,
                );
            }

            // Antigravity electric blue selection outline
            let rect_path =
                Path::rectangle(Point::new(sel.x, sel.y), Size::new(sel.width, sel.height));
            frame.stroke(
                &rect_path,
                Stroke {
                    width: 2.0,
                    style: canvas::Style::Solid(Color::from_rgb8(91, 155, 255)),
                    ..Stroke::default()
                },
            );
        } else {
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), mask_color);
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::Crosshair
    }
}

pub fn view_snip_overlay<'a, M: Clone + 'a>(
    state: &'a SnipState,
    hint_text: String,
    map_msg: impl Fn(SnipEvent) -> M + 'a,
) -> Element<'a, M> {
    let bg_img = image(state.preview_handle.clone())
        .width(Length::Fill)
        .height(Length::Fill)
        .content_fit(ContentFit::Fill);

    let canvas_layer: Element<'a, SnipEvent> = canvas::Canvas::new(SnipCanvas {
        selection: state.selection_rect(),
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into();
    let canvas_layer = canvas_layer.map(map_msg);

    let dims_label = if let Some(sel) = state.selection_rect() {
        format!(
            "{} × {} px",
            sel.width.round() as u32,
            sel.height.round() as u32
        )
    } else {
        hint_text
    };

    let hint_pill = container(
        container(
            text(dims_label)
                .size(13)
                .color(Color::from_rgb8(242, 245, 250)),
        )
        .padding([8, 16])
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgba8(15, 19, 30, 0.90))),
            border: Border {
                color: Color::from_rgba8(91, 155, 255, 0.55),
                width: 1.0,
                radius: 999.0.into(),
            },
            ..Default::default()
        }),
    )
    .width(Length::Fill)
    .padding(20)
    .center_x(Length::Fill);

    stack![bg_img, canvas_layer, hint_pill]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
