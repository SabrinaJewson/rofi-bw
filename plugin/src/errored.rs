pub(crate) struct Errored {
    message: String,
}

impl Errored {
    pub(crate) fn new(error: anyhow::Error) -> Self {
        let escaped = glib::markup_escape_text(&*format!("{error:?}"));
        let message = format!("<span foreground='red'>Error:</span> {escaped}");

        Self { message }
    }

    pub(crate) const DISPLAY_NAME: &'static str = "Error";

    pub(crate) fn message(&self) -> &str {
        &*self.message
    }
}

use rofi_mode::pango::glib;
