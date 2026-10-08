use super::*;

impl Workspace {
    pub(super) fn export_dialog(&mut self, bundle: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else { return; };
        let service = tab.service.commands.clone();
        let directory = tab.model.project.clone();
        let path = cx.prompt_for_new_path(&directory, Some(if bundle { "notebook.klbundle" } else { "notebook.ipynb" }));
        cx.spawn_in(window, async move |view, window| {
            match path.await {
                Ok(Ok(Some(path))) => {
                    let method = if bundle { "bundle_export" } else { "export_ipynb" };
                    let _ = service.send(Command::Inspect { method: method.into(), fields: json!({"path":path}) });
                    let _ = view.update_in(window, |v, _, cx| { v.select_surface("Diagnostics", cx); });
                }
                Ok(Err(error)) => { let _ = view.update_in(window, |v, _, cx| {
                    if let Some(tab) = v.tabs.get_mut(v.active) { tab.error = Some(format!("The export location could not be selected: {error}")); }
                    cx.notify();
                }); }
                _ => {}
            }
        }).detach();
    }
}
