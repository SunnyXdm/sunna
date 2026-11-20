// A successful stop promises a final callback. If the callback already
// arrived, a subsequent error needs no retry.
pub(super) fn stop_requested(mut stop: impl FnMut() -> bool, stopped: impl Fn() -> bool) -> bool {
    stop() || stopped() || stop() || stopped()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_error_without_callback_is_bounded() {
        let mut calls = 0;
        assert!(!stop_requested(
            || {
                calls += 1;
                false
            },
            || false
        ));
        assert_eq!(calls, 2);
        assert!(stop_requested(|| false, || true));
        assert!(stop_requested(
            || {
                calls -= 1;
                calls == 0
            },
            || false
        ));
    }
}
