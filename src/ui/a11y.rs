use accesskit::ActionRequest;

#[derive(Default)]
pub(crate) struct ActionHandler {}

impl accesskit::ActionHandler for ActionHandler {
    fn do_action(&mut self, _request: ActionRequest) {
        // Action requests from assistive technologies are not supported at the
        // moment.
    }
}

#[derive(Default)]
pub(crate) struct DeactivationHandler {}

impl accesskit::DeactivationHandler for DeactivationHandler {
    fn deactivate_accessibility(&mut self) {
        // There is currently no need to do anything when accessibility is
        // deactivated.
    }
}
