use super::*;

impl SettingsView {
    pub(super) fn row(
        &self,
        title: SharedString,
        detail: SharedString,
        muted: gpui::Hsla,
        small: Pixels,
        action: gpui::AnyElement,
    ) -> Setting {
        // The deck measures this row from `standard_height`, so both lines pin themselves to
        // that leading and truncate to one, and an empty detail still keeps its line.
        let detail_line = px((small / px(1.) * LEADING).round());
        let element = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .py_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .line_height(relative(LEADING))
                            .child(title.clone()),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .min_h(detail_line)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .line_height(relative(LEADING))
                            .text_color(muted)
                            .text_size(small)
                            .child(detail.clone()),
                    ),
            )
            .child(div().flex_none().child(action))
            .into_any_element();

        Setting {
            title,
            detail,
            element,
        }
    }
}

/// How many lines `text` wraps to at `width` in the window's font at `size`. Falls back to
/// one when the text cannot be shaped.
pub(super) fn wrapped_lines(
    text: SharedString,
    size: Pixels,
    width: Pixels,
    window: &Window,
) -> usize {
    let run = window.text_style().to_run(text.len());
    window
        .text_system()
        .shape_text(text, size, &[run], Some(width), None)
        .map(|lines| {
            lines
                .iter()
                .map(|line| line.wrap_boundaries().len() + 1)
                .sum()
        })
        .unwrap_or(1)
}
