use crate::backend::{Backend, Command, LinkAction, MouseButton, RenderableContent};
use crate::bindings::{BindingAction, BindingsLayout, InputKind};
use crate::terminal::{Event, Terminal};
use crate::theme::TerminalStyle;
use alacritty_terminal::index::Point as TerminalGridPoint;
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::{cell, TermMode};
use alacritty_terminal::vte::ansi::{self as ansi, NamedColor};
use iced::alignment::Vertical;
use iced::font::{Style as FontStyle, Weight as FontWeight};
use iced::mouse::{Cursor, ScrollDelta};
use iced::widget::container;
use iced::{Border, Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_core::clipboard::Kind as ClipboardKind;
use iced_core::keyboard::{Key, Modifiers};
use iced_core::mouse::{self, Click};
use iced_core::renderer::Quad;
use iced_core::text::{
    Alignment as TextAlignment, LineHeight, Renderer as _, Shaping, Text as CoreText, Wrapping,
};
use iced_core::widget::operation::{self, Focusable};
use iced_core::Renderer as _;
use iced_graphics::core::widget::{tree, Tree};
use iced_graphics::core::Widget;

pub struct TerminalView<'a> {
    term: &'a Terminal,
}

impl<'a> TerminalView<'a> {
    pub fn show(term: &'a Terminal) -> Element<'a, Event> {
        container(Self { term })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| term.theme.container_style())
            .into()
    }

    pub fn focus<Message: 'static>(id: iced::widget::Id) -> iced::Task<Message> {
        iced::widget::operation::focus(id)
    }

    fn is_cursor_in_layout(&self, cursor: Cursor, layout: iced_graphics::core::Layout<'_>) -> bool {
        if let Some(cursor_position) = cursor.position() {
            let layout_position = layout.position();
            let layout_size = layout.bounds();
            let is_triggered = cursor_position.x >= layout_position.x
                && cursor_position.y >= layout_position.y
                && cursor_position.x < (layout_position.x + layout_size.width)
                && cursor_position.y < (layout_position.y + layout_size.height);

            return is_triggered;
        }

        false
    }

    fn is_cursor_hovered_hyperlink(&self, state: &TerminalViewState) -> bool {
        let content = self.term.backend.renderable_content();
        if let Some(hyperlink_range) = &content.hovered_hyperlink {
            return hyperlink_range.contains(&state.mouse_position_on_grid);
        }

        false
    }

    fn handle_resize(
        &mut self,
        state: &mut TerminalViewState,
        layout: iced_graphics::core::Layout<'_>,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) {
        let layout_size = layout.bounds().size();
        if state.size != layout_size {
            state.size = layout_size;
            let cmd = Command::Resize(Some(layout_size), Some(self.term.font.measure));
            shell.publish(Event::BackendCall(self.term.id, cmd));
        }
    }

    fn handle_focus(
        &self,
        event: &iced_core::Event,
        state: &mut TerminalViewState,
        is_cursor_in_layout: bool,
    ) {
        use iced::Event::Mouse;
        use iced_core::mouse::{Button::Left, Event::ButtonPressed};

        if let Mouse(ButtonPressed(Left)) = event {
            state.focus = is_cursor_in_layout;
        }
    }

    fn handle_mouse_event(
        &self,
        state: &mut TerminalViewState,
        layout_position: Point,
        cursor_position: Point,
        event: &iced::mouse::Event,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let terminal_content = self.term.backend.renderable_content();
        let terminal_mode = terminal_content.terminal_mode;

        match event {
            iced_core::mouse::Event::ButtonPressed(iced_core::mouse::Button::Left) => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_left_button_pressed(
                    state,
                    &terminal_mode,
                    cursor_position,
                    layout_position,
                    &mut commands,
                );
            }
            iced_core::mouse::Event::CursorMoved { position } => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_cursor_moved(
                    state,
                    self.term.backend.renderable_content(),
                    position,
                    layout_position,
                    &mut commands,
                );
            }
            iced_core::mouse::Event::ButtonReleased(iced_core::mouse::Button::Left) => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_button_released(
                    state,
                    &terminal_mode,
                    &self.term.bindings,
                    &mut commands,
                );
            }
            iced::mouse::Event::WheelScrolled { delta } => {
                Self::handle_wheel_scrolled(state, *delta, &self.term.font.measure, &mut commands);
            }
            _ => {}
        }

        commands
    }

    fn handle_left_button_pressed(
        state: &mut TerminalViewState,
        terminal_mode: &TermMode,
        cursor_position: Point,
        layout_position: Point,
        commands: &mut Vec<Command>,
    ) {
        let cmd = if terminal_mode.intersects(TermMode::MOUSE_MODE) {
            Command::MouseReport(
                MouseButton::LeftButton,
                state.keyboard_modifiers,
                state.mouse_position_on_grid,
                true,
            )
        } else {
            let current_click = Click::new(cursor_position, mouse::Button::Left, state.last_click);
            let selection_type = match current_click.kind() {
                mouse::click::Kind::Single => SelectionType::Simple,
                mouse::click::Kind::Double => SelectionType::Semantic,
                mouse::click::Kind::Triple => SelectionType::Lines,
            };
            state.last_click = Some(current_click);
            Command::SelectStart(
                selection_type,
                (
                    cursor_position.x - layout_position.x,
                    cursor_position.y - layout_position.y,
                ),
            )
        };
        commands.push(cmd);
        state.is_dragged = true;
    }

    fn handle_cursor_moved(
        state: &mut TerminalViewState,
        terminal_content: &RenderableContent,
        position: &Point,
        layout_position: Point,
        commands: &mut Vec<Command>,
    ) {
        let cursor_x = position.x - layout_position.x;
        let cursor_y = position.y - layout_position.y;
        state.mouse_position_on_grid = Backend::selection_point(
            cursor_x,
            cursor_y,
            &terminal_content.terminal_size,
            terminal_content.grid.display_offset(),
        );

        // Handle command or selection update based on terminal mode and modifiers
        if state.is_dragged {
            let terminal_mode = terminal_content.terminal_mode;
            let cmd = if terminal_mode.intersects(TermMode::MOUSE_MOTION) {
                Command::MouseReport(
                    MouseButton::LeftMove,
                    state.keyboard_modifiers,
                    state.mouse_position_on_grid,
                    true,
                )
            } else {
                Command::SelectUpdate((cursor_x, cursor_y))
            };
            commands.push(cmd);
        }

        // Handle link hover if applicable
        if state.keyboard_modifiers == Modifiers::COMMAND {
            commands.push(Command::ProcessLink(
                LinkAction::Hover,
                state.mouse_position_on_grid,
            ));
        }
    }

    fn handle_button_released(
        state: &mut TerminalViewState,
        terminal_mode: &TermMode,
        bindings: &BindingsLayout, // Use the actual type of your bindings here
        commands: &mut Vec<Command>,
    ) {
        state.is_dragged = false;

        if terminal_mode.intersects(TermMode::MOUSE_MODE) {
            commands.push(Command::MouseReport(
                MouseButton::LeftButton,
                state.keyboard_modifiers,
                state.mouse_position_on_grid,
                false,
            ));
        }

        if bindings.get_action(
            InputKind::Mouse(iced_core::mouse::Button::Left),
            state.keyboard_modifiers,
            *terminal_mode,
        ) == BindingAction::LinkOpen
        {
            commands.push(Command::ProcessLink(
                LinkAction::Open,
                state.mouse_position_on_grid,
            ));
        }
    }

    fn handle_wheel_scrolled(
        state: &mut TerminalViewState,
        delta: ScrollDelta,
        font_measure: &Size<f32>,
        commands: &mut Vec<Command>,
    ) {
        match delta {
            ScrollDelta::Lines { y, .. } => {
                let lines = y.signum() * y.abs().round();
                commands.push(Command::Scroll(lines as i32));
            }
            ScrollDelta::Pixels { y, .. } => {
                state.scroll_pixels -= y;
                let line_height = font_measure.height; // Assume this method exists and gives the height of a line
                let lines = (state.scroll_pixels / line_height).trunc();
                state.scroll_pixels %= line_height;
                if lines != 0.0 {
                    commands.push(Command::Scroll(lines as i32));
                }
            }
        }
    }

    fn handle_keyboard_event(
        &self,
        state: &mut TerminalViewState,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        event: &iced::keyboard::Event,
    ) -> Option<Command> {
        let mut binding_action = BindingAction::Ignore;
        let last_content = self.term.backend.renderable_content();
        match event {
            iced::keyboard::Event::ModifiersChanged(m) => {
                state.keyboard_modifiers = *m;
                let action = if state.keyboard_modifiers == Modifiers::COMMAND {
                    LinkAction::Hover
                } else {
                    LinkAction::Clear
                };
                return Some(Command::ProcessLink(action, state.mouse_position_on_grid));
            }
            iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            } => match &key {
                // Use the physical character key for bindings even when text is None (e.g., Ctrl/Cmd combos)
                Key::Character(k) => {
                    let lower = k.to_ascii_lowercase();
                    binding_action = self.term.bindings.get_action(
                        InputKind::Char(lower),
                        state.keyboard_modifiers,
                        last_content.terminal_mode,
                    );

                    // If no binding matched, only write printable text (when provided)
                    if binding_action == BindingAction::Ignore {
                        if let Some(c) = text {
                            return Some(Command::Write(c.as_bytes().to_vec()));
                        }
                    }
                }
                Key::Named(code) => {
                    binding_action = self.term.bindings.get_action(
                        InputKind::KeyCode(*code),
                        *modifiers,
                        last_content.terminal_mode,
                    );
                }
                _ => {}
            },
            _ => {}
        }

        match binding_action {
            BindingAction::Char(c) => {
                let mut buf = [0, 0, 0, 0];
                let str = c.encode_utf8(&mut buf);
                return Some(Command::Write(str.as_bytes().to_vec()));
            }
            BindingAction::Esc(seq) => {
                return Some(Command::Write(seq.as_bytes().to_vec()));
            }
            BindingAction::Paste => {
                if let Some(data) = clipboard.read(ClipboardKind::Standard) {
                    let input: Vec<u8> = data.bytes().collect();
                    return Some(Command::Write(input));
                }
            }
            BindingAction::Copy => {
                clipboard.write(
                    ClipboardKind::Standard,
                    self.term.backend.selectable_content(),
                );
            }
            _ => {}
        };

        None
    }
}

impl Widget<Event, Theme, iced::Renderer> for TerminalView<'_> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TerminalViewState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TerminalViewState::new())
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: iced_core::Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        let wid = self.term.widget_id();
        operation.focusable(Some(wid), layout.bounds(), state);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<TerminalViewState>();
        let content = self.term.backend.renderable_content();
        let term_size = content.terminal_size;
        let cell_width = term_size.cell_width;
        let cell_height = term_size.cell_height;
        if cell_width <= 0.0 || cell_height <= 0.0 {
            return;
        }

        let font_size = self.term.font.size;
        let line_height = LineHeight::Relative(self.term.font.scale_factor);
        let bounds = layout.bounds();
        let origin = bounds.position();
        let display_offset = content.grid.display_offset() as f32;
        let focused = state.is_focused();

        // The terminal background is painted by the widget itself (a solid quad
        // under the grid), so the embedding container no longer needs an opaque
        // shim and the surrounding window can stay translucent glass.
        let default_bg = self
            .term
            .theme
            .get_color(ansi::Color::Named(NamedColor::Background));

        let cell_rect = |col: f32, line: f32| Rectangle {
            x: origin.x + col * cell_width,
            y: origin.y + (line + display_offset) * cell_height,
            width: cell_width,
            height: cell_height,
        };

        // Draw at absolute layout coordinates, clipped to the widget bounds —
        // the same native text/quad path the rest of the UI uses. There is no
        // separate canvas-geometry layer to mis-position, get clipped away when
        // embedded off-origin, or vanish over a transparent surface. Within a
        // layer iced draws quads beneath text, so backgrounds/cursor land under
        // the glyphs automatically.
        renderer.with_layer(bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds,
                    ..Quad::default()
                },
                default_bg,
            );

            for indexed in content.grid.display_iter() {
                let point = indexed.point;
                let col = point.column.0 as f32;
                let line = point.line.0 as f32;
                let rect = cell_rect(col, line);
                let flags = indexed.cell.flags;

                // Resolve colors. Bold promotes the 8 normal ANSI colors to
                // their bright variants (conventional "bold = bright"), a big
                // part of making output read as vivid rather than muted.
                let fg_ansi = if flags.intersects(cell::Flags::BOLD) {
                    brighten(indexed.fg)
                } else {
                    indexed.fg
                };
                let mut fg = self.term.theme.get_color(fg_ansi);
                let mut bg = self.term.theme.get_color(indexed.bg);

                if flags.intersects(cell::Flags::DIM | cell::Flags::DIM_BOLD) {
                    fg.a *= 0.7;
                }
                let selected = content.selectable_range.is_some_and(|r| r.contains(point));
                if flags.contains(cell::Flags::INVERSE) || selected {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Cell background (skip default; the backing quad covers it).
                if bg != default_bg {
                    renderer.fill_quad(
                        Quad {
                            bounds: rect,
                            ..Quad::default()
                        },
                        bg,
                    );
                }

                // Cursor: a solid block when focused (the glyph is re-drawn in
                // the background color so it stays legible), a hollow outline
                // when not.
                let is_cursor = content.grid.cursor.point == point
                    && content.terminal_mode.contains(TermMode::SHOW_CURSOR);
                if is_cursor {
                    let cursor_color = self.term.theme.get_color(content.cursor.fg);
                    if focused {
                        renderer.fill_quad(
                            Quad {
                                bounds: rect,
                                ..Quad::default()
                            },
                            cursor_color,
                        );
                        fg = default_bg;
                    } else {
                        renderer.fill_quad(
                            Quad {
                                bounds: rect,
                                border: Border {
                                    color: cursor_color,
                                    width: 1.0,
                                    radius: 0.0.into(),
                                },
                                ..Quad::default()
                            },
                            Color::TRANSPARENT,
                        );
                    }
                }

                // Underline (cell attribute or a hovered ⌘-hyperlink).
                let underline = flags.contains(cell::Flags::UNDERLINE)
                    || content.hovered_hyperlink.as_ref().is_some_and(|range| {
                        range.contains(&point) && range.contains(&state.mouse_position_on_grid)
                    });
                if underline {
                    let thickness = (font_size * 0.08).max(1.0);
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: rect.x,
                                y: rect.y + rect.height - thickness,
                                width: rect.width,
                                height: thickness,
                            },
                            ..Quad::default()
                        },
                        fg,
                    );
                }

                // Glyph (centered in the cell; cell width is the font's true
                // advance, so columns line up without squish).
                let c = indexed.c;
                if c != ' ' && c != '\t' {
                    let mut font = self.term.font.font_type;
                    if flags.intersects(cell::Flags::BOLD | cell::Flags::DIM_BOLD) {
                        font.weight = FontWeight::Bold;
                    }
                    if flags.contains(cell::Flags::ITALIC) {
                        font.style = FontStyle::Italic;
                    }
                    let text = CoreText {
                        content: c.to_string(),
                        bounds: Size::new(cell_width, cell_height),
                        size: iced_core::Pixels(font_size),
                        line_height,
                        font,
                        align_x: TextAlignment::Center,
                        align_y: Vertical::Center,
                        shaping: Shaping::Advanced,
                        wrapping: Wrapping::None,
                    };
                    let position =
                        Point::new(rect.x + cell_width / 2.0, rect.y + cell_height / 2.0);
                    renderer.fill_text(text, position, fg, bounds);
                }
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: iced_graphics::core::Layout<'_>,
        cursor: Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        self.handle_resize(state, layout, shell);

        let is_cursor_in_layout = self.is_cursor_in_layout(cursor, layout);
        self.handle_focus(event, state, is_cursor_in_layout);

        let commands = match event {
            iced::Event::Mouse(mouse_event) if is_cursor_in_layout => self.handle_mouse_event(
                state,
                layout.position(),
                cursor.position().unwrap(),
                mouse_event,
            ),
            iced::Event::Keyboard(keyboard_event) => {
                if !state.is_focused() {
                    return;
                }

                self.handle_keyboard_event(state, clipboard, keyboard_event)
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        };

        if !commands.is_empty() {
            shell.capture_event();
        }

        for cmd in commands {
            shell.publish(Event::BackendCall(self.term.id, cmd));
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> iced_core::mouse::Interaction {
        let state = tree.state.downcast_ref::<TerminalViewState>();
        let mut cursor_mode = iced_core::mouse::Interaction::Idle;
        let terminal_mode = self.term.backend.renderable_content().terminal_mode;
        if self.is_cursor_in_layout(cursor, layout) && !terminal_mode.contains(TermMode::SGR_MOUSE)
        {
            cursor_mode = iced_core::mouse::Interaction::Text;
        }

        if self.is_cursor_hovered_hyperlink(state) {
            cursor_mode = iced_core::mouse::Interaction::Pointer;
        }

        cursor_mode
    }
}

impl<'a> From<TerminalView<'a>> for Element<'a, Event, Theme, iced::Renderer> {
    fn from(widget: TerminalView<'a>) -> Self {
        Self::new(widget)
    }
}

#[derive(Debug, Clone)]
struct TerminalViewState {
    focus: bool,
    is_dragged: bool,
    last_click: Option<mouse::Click>,
    scroll_pixels: f32,
    keyboard_modifiers: Modifiers,
    size: Size<f32>,
    mouse_position_on_grid: TerminalGridPoint,
}

impl TerminalViewState {
    fn new() -> Self {
        Self {
            focus: false,
            is_dragged: false,
            last_click: None,
            scroll_pixels: 0.0,
            keyboard_modifiers: Modifiers::empty(),
            size: Size::from([0.0, 0.0]),
            mouse_position_on_grid: TerminalGridPoint::default(),
        }
    }
}

impl Default for TerminalViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl operation::Focusable for TerminalViewState {
    fn is_focused(&self) -> bool {
        self.focus
    }

    fn focus(&mut self) {
        self.focus = true;
    }

    fn unfocus(&mut self) {
        self.focus = false;
    }
}

/// Promote the 8 normal ANSI colors to their bright counterparts (used for bold
/// text). Bright/dim/foreground named colors, the 256-color cube, and truecolor
/// are returned unchanged.
fn brighten(color: ansi::Color) -> ansi::Color {
    use NamedColor::*;
    match color {
        ansi::Color::Named(name) => ansi::Color::Named(match name {
            Black => BrightBlack,
            Red => BrightRed,
            Green => BrightGreen,
            Yellow => BrightYellow,
            Blue => BrightBlue,
            Magenta => BrightMagenta,
            Cyan => BrightCyan,
            White => BrightWhite,
            other => other,
        }),
        ansi::Color::Indexed(i) if i < 8 => ansi::Color::Indexed(i + 8),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod handle_left_button_pressed_tests {
        use super::*;
        use alacritty_terminal::index::{Column, Line};

        #[test]
        fn handles_mouse_mode_with_left_click() {
            let mut state = TerminalViewState::new();
            let terminal_mode = TermMode::MOUSE_MODE;
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_left_button_pressed(
                &mut state,
                &terminal_mode,
                cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0),
                    },
                    true,
                )
            ));
            assert!(state.is_dragged);
        }

        #[test]
        fn starts_simple_selection_with_left_click() {
            let terminal_mode = TermMode::SGR_MOUSE;
            let cursor_position = Point { x: 200.0, y: 150.0 };
            let layout_position = Point { x: 50.0, y: 50.0 };

            let cases = vec![
                SelectionType::Simple,
                SelectionType::Semantic,
                SelectionType::Lines,
            ];

            for _selection_type in cases {
                let mut state = TerminalViewState::new();
                state.keyboard_modifiers = Modifiers::SHIFT;
                let mut commands = Vec::new();

                TerminalView::handle_left_button_pressed(
                    &mut state,
                    &terminal_mode,
                    cursor_position,
                    layout_position,
                    &mut commands,
                );

                assert_eq!(commands.len(), 1);
                assert!(matches!(
                    commands[0],
                    Command::SelectStart(_selection_type, (150.0, 100.0))
                ),);
                assert!(state.is_dragged);
            }
        }
    }

    mod handle_cursor_moved_tests {
        use alacritty_terminal::index::{Column, Line};

        use super::*;

        #[test]
        fn updates_mouse_position_on_grid() {
            let mut state = TerminalViewState::new();
            let terminal_content = RenderableContent::default();
            let mut commands = Vec::new();
            let cases = vec![
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 1.0, y: 1.0 },
                    TerminalGridPoint {
                        line: Line(1),
                        column: Column(1),
                    },
                ),
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 2.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(2),
                    },
                ),
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(30),
                    },
                ),
                (
                    Point { x: 10.0, y: 0.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(20),
                    },
                ),
                (
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(20),
                    },
                ),
            ];

            for (layout_position, cursor_position, expected) in cases {
                TerminalView::handle_cursor_moved(
                    &mut state,
                    &terminal_content,
                    &cursor_position,
                    layout_position,
                    &mut commands,
                );

                assert_eq!(state.mouse_position_on_grid, expected);
            }
        }

        #[test]
        fn generates_drag_update_command_when_dragged() {
            let mut state = TerminalViewState::new();
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent::default();
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::SelectUpdate((95.0, 145.0))));
        }

        #[test]
        fn generates_drag_update_command_when_dragged_in_mouse_motion_mode() {
            let mut state = TerminalViewState::new();
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::MOUSE_MOTION,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftMove,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(49),
                        column: Column(79),
                    },
                    true,
                )
            ));
        }

        #[test]
        fn generates_drag_update_command_when_dragged_in_srg_mode_with_key_mods() {
            let mut state = TerminalViewState::new();
            state.keyboard_modifiers = Modifiers::SHIFT;
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::SGR_MOUSE,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::SelectUpdate((95.0, 145.0))));
        }

        #[test]
        fn generates_drag_update_and_link_open() {
            let mut state = TerminalViewState::new();
            state.keyboard_modifiers = Modifiers::COMMAND;
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::SGR_MOUSE,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 2);
            assert!(matches!(commands[0], Command::SelectUpdate((95.0, 145.0))));
            assert!(matches!(
                commands[1],
                Command::ProcessLink(
                    LinkAction::Hover,
                    TerminalGridPoint {
                        line: Line(49),
                        column: Column(79),
                    },
                )
            ));
        }
    }

    mod handle_button_released_tests {
        use super::*;
        use alacritty_terminal::index::{Column, Line};

        #[test]
        fn mouse_mode_activated() {
            let mut state = TerminalViewState::new();
            let terminal_mode = TermMode::MOUSE_MODE;
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    },
                    false
                )
            ));
        }

        #[test]
        fn link_open_on_button_release() {
            let mut state = TerminalViewState::new();
            state.keyboard_modifiers = Modifiers::COMMAND;
            let terminal_mode = TermMode::MOUSE_MODE;
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 2);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    },
                    false
                )
            ));
            assert!(matches!(
                commands[1],
                Command::ProcessLink(
                    LinkAction::Open,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    }
                ),
            ));
        }

        #[test]
        fn link_open_on_button_release_in_non_mouse_mode() {
            let mut state = TerminalViewState::new();
            state.keyboard_modifiers = Modifiers::COMMAND;
            state.mouse_position_on_grid = TerminalGridPoint {
                line: Line(4),
                column: Column(10),
            };
            let terminal_mode = TermMode::empty(); // Assume SGR_MOUSE mode doesn't affect link opening
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::ProcessLink(
                    LinkAction::Open,
                    TerminalGridPoint {
                        line: Line(4),
                        column: Column(10)
                    }
                ),
            ));
        }
    }

    mod handle_wheel_scrolled_tests {
        use super::*;
        use crate::font::TermFont;
        use crate::settings::FontSettings;

        #[test]
        fn scroll_with_lines_downward() {
            let mut state = TerminalViewState::new();
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Lines { y: 3.0, x: 0.0 }, // Scroll down 3 lines
                &font.measure,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(3)));
        }

        #[test]
        fn scroll_with_lines_upward() {
            let mut state = TerminalViewState::new();
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Lines { y: -2.0, x: 0.0 },
                &font.measure,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(-2)));
        }

        #[test]
        fn scroll_with_pixels_accumulating_downward() {
            let mut state = TerminalViewState::new();
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Pixels { y: 45.0, x: 0.0 },
                &font.measure,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(-2)));
            assert_eq!(state.scroll_pixels, -8.600002);
        }

        #[test]
        fn scroll_with_pixels_accumulating_upward() {
            let mut state = TerminalViewState::new();
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Pixels { y: -60.0, x: 0.0 },
                &font.measure,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(3)));
            assert_eq!(state.scroll_pixels, 5.4000034);
        }
    }
}
