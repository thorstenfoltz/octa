//! Files another `octa <file>` start handed over (`octa::single_instance`).

use eframe::egui;

use super::state::OctaApp;

impl OctaApp {
    /// Open handed-over files like command-line files and bring the window
    /// forward. Windows' foreground rules and Wayland's activation protocol
    /// may refuse the focus; the taskbar entry then flashes instead.
    pub(crate) fn drain_instance_handover(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.attention_pending)
            && ctx.input(|i| i.viewport().focused) == Some(false)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
        let Some(listener) = &self.instance else {
            return;
        };
        let mut paths = Vec::new();
        while let Ok(batch) = listener.rx.try_recv() {
            paths.extend(batch);
        }
        if paths.is_empty() {
            return;
        }
        self.enqueue_open_files(paths);
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.attention_pending = true;
        ctx.request_repaint();
    }
}
