pub(crate) fn report_error(error: &(dyn 'static + Error)) {
    let mut notification = notify_rust::Notification::new();
    notification.icon("bitwarden");
    notification.urgency(notify_rust::Urgency::Critical);
    notification.summary = format!("Error: {error}");

    for (i, error) in error_chain(error).enumerate().skip(1) {
        if notification.body.is_empty() {
            notification.body.push_str("Caused by:\n");
        }
        writeln!(notification.body, "{i:4}: {error}").unwrap();
    }

    eprintln!("{}\n\n{}", notification.summary, notification.body);

    drop(notification.show());
}

use std::error::Error;
use std::fmt::Write as _;

use error_chain::error_chain;
mod error_chain {
    pub(crate) fn error_chain<'error>(error: &'error (dyn 'static + Error)) -> Chain<'error> {
        Chain(Some(error))
    }

    pub(crate) struct Chain<'error>(Option<&'error (dyn 'static + Error)>);

    impl<'error> Iterator for Chain<'error> {
        type Item = &'error (dyn 'static + Error);

        fn next(&mut self) -> Option<Self::Item> {
            let current = self.0.take()?;
            self.0 = current.source();
            Some(current)
        }
    }

    use std::error::Error;
}
