use iced::widget::text::Span;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Color, Element, Font, Length, Padding};
use lilia_desktop_application::{
    DesktopTerminalColor, DesktopTerminalProcessState, DesktopTerminalScope,
    DesktopTerminalSessionId, DesktopTerminalSnapshot, DesktopTerminalStyle,
};
use nana_ui::widgets::{button_style, canvas_style, text_input_style, vertical_scrollbar};
use nana_ui::{ButtonKind, EmptyState, Icon, SelectableRichText, ThemeTokens};

#[derive(Clone, Debug)]
pub enum TerminalViewMessage {
    InputChanged(DesktopTerminalSessionId, String),
    Submit(DesktopTerminalSessionId),
    Interrupt(DesktopTerminalSessionId),
    Eof(DesktopTerminalSessionId),
    CopyVisible(DesktopTerminalSessionId),
    Resize(DesktopTerminalSessionId, u16, u16),
    Scroll(DesktopTerminalSessionId, usize),
    Terminate(DesktopTerminalSessionId),
    Reveal(DesktopTerminalSessionId),
    NewSession(DesktopTerminalScope),
}

pub fn terminal_content(
    snapshot: &DesktopTerminalSnapshot,
    input: &str,
    notice: Option<&str>,
    tokens: ThemeTokens,
) -> Element<'static, TerminalViewMessage> {
    let colors = tokens.colors;
    let status = process_label(&snapshot.process);
    let status_color = if snapshot.process.is_running() {
        colors.success
    } else {
        colors.muted
    };
    let mut actions = row![
        text(status).size(10).color(status_color),
        text(format!("{}×{}", snapshot.columns, snapshot.rows))
            .size(10)
            .color(colors.faint),
        text(snapshot.cwd.display().to_string())
            .size(10)
            .color(colors.muted),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    if snapshot.process.is_running() {
        let (next_rows, next_columns) = next_terminal_size(snapshot.rows, snapshot.columns);
        actions = actions
            .push(
                button(text("调整尺寸").size(10))
                    .on_press(TerminalViewMessage::Resize(
                        snapshot.id.clone(),
                        next_rows,
                        next_columns,
                    ))
                    .style(button_style(tokens, ButtonKind::Text)),
            )
            .push(
                button(text("终止").size(10).color(colors.danger))
                    .on_press(TerminalViewMessage::Terminate(snapshot.id.clone()))
                    .style(button_style(tokens, ButtonKind::Text)),
            );
    } else {
        actions = actions.push(
            button(text("新建终端").size(10))
                .on_press(TerminalViewMessage::NewSession(snapshot.scope.clone()))
                .style(button_style(tokens, ButtonKind::Primary)),
        );
    }

    let mut screen = column![].spacing(0).width(Length::Fill);
    for row in &snapshot.screen {
        screen = screen.push(
            SelectableRichText::new(terminal_spans(row, tokens))
                .size(12)
                .width(Length::Fill),
        );
    }
    let terminal = container(
        scrollable(screen)
            .direction(vertical_scrollbar())
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(Padding::from([10, 12]))
    .style(move |_theme| {
        iced::widget::container::Style::default()
            .background(colors.background)
            .border(iced::Border {
                color: colors.border_soft,
                width: 1.0,
                radius: 4.0.into(),
            })
    });

    let id = snapshot.id.clone();
    let submit_id = snapshot.id.clone();
    let input = text_input("输入命令", input)
        .id(crate::target_ids::terminal_input(snapshot.id.as_str()))
        .on_input(move |value| TerminalViewMessage::InputChanged(id.clone(), value))
        .on_submit(TerminalViewMessage::Submit(submit_id))
        .padding([7, 10])
        .size(12)
        .font(Font::MONOSPACE)
        .style(text_input_style(tokens, false));
    let input: Element<'static, TerminalViewMessage> = if snapshot.process.is_running() {
        input.into()
    } else {
        text("会话已结束；新建终端后可继续输入。")
            .size(11)
            .color(colors.muted)
            .into()
    };

    let mut footer = row![input].spacing(8).align_y(Alignment::Center);
    if snapshot.maximum_scrollback_position > 0 {
        let older = snapshot
            .scrollback_position
            .saturating_add(snapshot.rows as usize)
            .min(snapshot.maximum_scrollback_position);
        let newer = snapshot
            .scrollback_position
            .saturating_sub(snapshot.rows as usize);
        footer = footer
            .push(
                button(text("更早").size(10))
                    .on_press(TerminalViewMessage::Scroll(snapshot.id.clone(), older))
                    .style(button_style(tokens, ButtonKind::Text)),
            )
            .push(
                button(text("更新").size(10))
                    .on_press(TerminalViewMessage::Scroll(snapshot.id.clone(), newer))
                    .style(button_style(tokens, ButtonKind::Text)),
            );
    }
    let mut controls = row![].spacing(8).align_y(Alignment::Center);
    if snapshot.process.is_running() {
        controls = controls
            .push(
                button(text("Ctrl+C").size(10))
                    .on_press(TerminalViewMessage::Interrupt(snapshot.id.clone()))
                    .style(button_style(tokens, ButtonKind::Text)),
            )
            .push(
                button(text("Ctrl+D").size(10))
                    .on_press(TerminalViewMessage::Eof(snapshot.id.clone()))
                    .style(button_style(tokens, ButtonKind::Text)),
            );
    }
    controls = controls.push(
        button(text("复制输出").size(10))
            .on_press(TerminalViewMessage::CopyVisible(snapshot.id.clone()))
            .style(button_style(tokens, ButtonKind::Text)),
    );

    let mut content = column![actions, controls, terminal, footer].spacing(8);
    if let Some(notice) = notice {
        content = content.push(text(notice.to_owned()).size(10).color(colors.success));
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from([10, 12]))
        .style(canvas_style(tokens))
        .into()
}

pub fn terminal_plain_text(snapshot: &DesktopTerminalSnapshot) -> String {
    terminal_rows_plain_text(&snapshot.screen)
}

fn terminal_rows_plain_text(rows: &[lilia_desktop_application::DesktopTerminalRow]) -> String {
    rows.iter()
        .map(|row| row.text.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end_matches('\n')
        .to_owned()
}

#[cfg(test)]
mod tests {
    use lilia_desktop_application::DesktopTerminalRow;

    use super::terminal_rows_plain_text;

    #[test]
    fn copied_terminal_output_preserves_lines_and_omits_screen_padding() {
        let rows = vec![
            DesktopTerminalRow {
                text: "first   ".to_owned(),
                styles: Vec::new(),
            },
            DesktopTerminalRow {
                text: "second".to_owned(),
                styles: Vec::new(),
            },
            DesktopTerminalRow {
                text: "        ".to_owned(),
                styles: Vec::new(),
            },
        ];
        assert_eq!(terminal_rows_plain_text(&rows), "first\nsecond");
    }
}

pub fn terminal_inactive_preview(
    snapshot: &DesktopTerminalSnapshot,
    tokens: ThemeTokens,
) -> Element<'static, TerminalViewMessage> {
    let last_line = snapshot
        .screen
        .iter()
        .rev()
        .find(|row| !row.text.trim().is_empty())
        .map(|row| row.text.trim_end().to_owned());
    container(
        EmptyState::new(process_label(&snapshot.process))
            .message(last_line.unwrap_or_else(|| snapshot.cwd.display().to_string()))
            .icon(Icon::Workspace)
            .view(tokens),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center(Length::Fill)
    .style(canvas_style(tokens))
    .into()
}

fn terminal_spans(
    row: &lilia_desktop_application::DesktopTerminalRow,
    tokens: ThemeTokens,
) -> Vec<Span<'static, String, Font>> {
    if row.styles.is_empty() {
        return vec![Span::new(row.text.clone())
            .font(Font::MONOSPACE)
            .color(tokens.colors.text)];
    }
    row.styles
        .iter()
        .filter_map(|style| {
            row.text
                .get(style.start..style.end)
                .map(|value| terminal_span(value.to_owned(), style.style, tokens))
        })
        .collect()
}

fn terminal_span(
    value: String,
    style: DesktopTerminalStyle,
    tokens: ThemeTokens,
) -> Span<'static, String, Font> {
    let mut foreground = terminal_color(style.foreground, tokens.colors.text);
    if style.inverse {
        foreground = terminal_color(style.background, tokens.colors.surface);
    }
    let weight = if style.bold {
        iced::font::Weight::Bold
    } else {
        iced::font::Weight::Normal
    };
    Span::new(value)
        .font(Font {
            weight,
            ..Font::MONOSPACE
        })
        .color(if style.dim {
            Color {
                a: 0.65,
                ..foreground
            }
        } else {
            foreground
        })
}

fn terminal_color(color: DesktopTerminalColor, default: Color) -> Color {
    match color {
        DesktopTerminalColor::Default => default,
        DesktopTerminalColor::Indexed(index) if index < 16 => BASIC_COLORS[index as usize],
        DesktopTerminalColor::Indexed(index) if index < 232 => {
            let value = index - 16;
            let red = value / 36;
            let green = (value % 36) / 6;
            let blue = value % 6;
            Color::from_rgb8(cube(red), cube(green), cube(blue))
        }
        DesktopTerminalColor::Indexed(index) => {
            let level = 8_u8.saturating_add((index - 232).saturating_mul(10));
            Color::from_rgb8(level, level, level)
        }
        DesktopTerminalColor::Rgb([red, green, blue]) => Color::from_rgb8(red, green, blue),
    }
}

fn cube(value: u8) -> u8 {
    if value == 0 {
        0
    } else {
        55_u8.saturating_add(value.saturating_mul(40))
    }
}

fn process_label(process: &DesktopTerminalProcessState) -> &'static str {
    match process {
        DesktopTerminalProcessState::Running => "运行中",
        DesktopTerminalProcessState::Terminating => "正在终止",
        DesktopTerminalProcessState::Exited { success: true, .. } => "已完成",
        DesktopTerminalProcessState::Exited { success: false, .. } => "已退出",
        DesktopTerminalProcessState::Failed { .. } => "启动失败",
        DesktopTerminalProcessState::Restored => "已结束",
    }
}

fn next_terminal_size(rows: u16, columns: u16) -> (u16, u16) {
    if rows < 36 || columns < 120 {
        (36, 120)
    } else {
        (24, 80)
    }
}

const BASIC_COLORS: [Color; 16] = [
    Color::from_rgb(0.0, 0.0, 0.0),
    Color::from_rgb(0.8, 0.2, 0.2),
    Color::from_rgb(0.2, 0.7, 0.3),
    Color::from_rgb(0.8, 0.65, 0.2),
    Color::from_rgb(0.25, 0.45, 0.85),
    Color::from_rgb(0.7, 0.3, 0.75),
    Color::from_rgb(0.2, 0.7, 0.75),
    Color::from_rgb(0.75, 0.75, 0.75),
    Color::from_rgb(0.35, 0.35, 0.35),
    Color::from_rgb(1.0, 0.35, 0.35),
    Color::from_rgb(0.35, 0.9, 0.45),
    Color::from_rgb(1.0, 0.85, 0.35),
    Color::from_rgb(0.4, 0.6, 1.0),
    Color::from_rgb(0.95, 0.45, 1.0),
    Color::from_rgb(0.35, 0.9, 0.95),
    Color::from_rgb(1.0, 1.0, 1.0),
];
