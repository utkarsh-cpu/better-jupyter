//! Native equation layout for the declared inert math subset.
use super::*;
use kernelledger_client::math::MathNode;

fn node(value: &MathNode, size: f32, cx: &App) -> AnyElement {
    match value {
        MathNode::Text(text) => div()
            .text_size(px(size))
            .child(text.clone())
            .into_any_element(),
        MathNode::Row(nodes) => h_flex()
            .items_center()
            .children(nodes.iter().map(|n| node(n, size, cx)))
            .into_any_element(),
        MathNode::Fraction(top, bottom) => v_flex()
            .items_center()
            .px_1()
            .child(
                div()
                    .px_1()
                    .pb_0p5()
                    .child(node(top, (size * 0.9).max(10.), cx)),
            )
            .child(
                div()
                    .w_full()
                    .border_t_1()
                    .border_color(cx.theme().foreground),
            )
            .child(
                div()
                    .px_1()
                    .pt_0p5()
                    .child(node(bottom, (size * 0.9).max(10.), cx)),
            )
            .into_any_element(),
        MathNode::Root(value) => h_flex()
            .items_center()
            .child(div().text_size(px(size * 1.3)).child("√"))
            .child(
                div()
                    .border_t_1()
                    .border_color(cx.theme().foreground)
                    .px_0p5()
                    .child(node(value, size, cx)),
            )
            .into_any_element(),
        MathNode::Script { base, upper, lower } => h_flex()
            .items_center()
            .child(node(base, size, cx))
            .child(
                v_flex()
                    .gap_0p5()
                    .when_some(upper.as_ref(), |d, upper| {
                        d.child(node(upper, (size * 0.7).max(10.), cx))
                    })
                    .when_some(lower.as_ref(), |d, lower| {
                        d.child(node(lower, (size * 0.7).max(10.), cx))
                    }),
            )
            .into_any_element(),
    }
}

pub(super) fn equation(source: &str, id: String, cx: &App) -> AnyElement {
    match kernelledger_client::math::parse(source) {
        Ok(tree) => div()
            .id(SharedString::from(id))
            .role(Role::Group)
            .aria_label(format!("Equation: {}", tree.description()))
            .overflow_x_scroll()
            .max_h(px(360.))
            .overflow_y_scroll()
            .py_2()
            .child(node(&tree, 20., cx))
            .into_any_element(),
        Err(error) => v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Equation shown as source: {error}")),
            )
            .child(
                TextView::markdown(
                    SharedString::from(id),
                    format!(
                        "```text\n{}\n```",
                        bounded(source, 65536).replace("```", "~~~")
                    ),
                )
                .selectable(true),
            )
            .into_any_element(),
    }
}
