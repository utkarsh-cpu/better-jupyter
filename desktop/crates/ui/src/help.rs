use super::*;

impl Workspace {
    pub(super) fn help_content(&self, cx: &mut Context<Self>) -> Div {
        let query = self.help_query.read(cx).value().to_lowercase();
        let source = include_str!("../../../../docs/desktop/help.md");
        let content = if query.trim().is_empty() {
            source.to_owned()
        } else {
            let sections = source
                .split("\n## ")
                .enumerate()
                .filter(|(_, section)| section.to_lowercase().contains(query.trim()))
                .map(|(index, section)| {
                    if index == 0 {
                        section.to_string()
                    } else {
                        format!("## {section}")
                    }
                })
                .collect::<Vec<_>>();
            if sections.is_empty() {
                "No matching help topics. Try ‘run’, ‘checkpoint’, or ‘recovery’.".into()
            } else {
                sections.join("\n\n")
            }
        };
        v_flex()
            .max_w(px(760.))
            .gap_3()
            .child(Input::new(&self.help_query).cleanable(true))
            .child(TextView::markdown("task-help", content).selectable(true))
    }
}
