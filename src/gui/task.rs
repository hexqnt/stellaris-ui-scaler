use std::{sync::mpsc, time::Duration};

use eframe::egui;

use crate::compiler::Control;

pub(super) struct Task<T> {
    receiver: mpsc::Receiver<T>,
    pub(super) control: Control,
}

impl<T: Send + 'static> Task<T> {
    pub(super) fn spawn(
        ctx: &egui::Context,
        work: impl FnOnce(Control, &mpsc::Sender<T>) -> T + Send + 'static,
    ) -> Self {
        let (sender, task) = Self::channel();
        let worker_control = task.control.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = work(worker_control, &sender);
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        task
    }

    pub(super) fn try_recv(&self, ctx: &egui::Context) -> Result<T, mpsc::TryRecvError> {
        let result = self.receiver.try_recv();
        if matches!(result, Err(mpsc::TryRecvError::Empty)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        result
    }

    pub(super) fn channel() -> (mpsc::Sender<T>, Self) {
        let (sender, receiver) = mpsc::channel();
        (
            sender,
            Self {
                receiver,
                control: Control::default(),
            },
        )
    }
}

impl<T> Drop for Task<T> {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
