macro_rules! trace_field {
    (_impl pre $field:expr, $enc:expr, $signed:expr, $raw:expr) => {
        tracing::trace!(
            field = $field.name(),
            encoding = ?$enc,
            signed_encoding = $signed,
            raw = $raw,
        );
    };
    (_impl post $field:expr, $pred:expr, $signed:expr, $final:expr) => {
        tracing::trace!(
            field = $field.name(),
            predictor = ?$pred,
            signed = $signed,
            value = $final,
        );
    };

    (pre, field = $field:expr, enc = $enc:expr, raw = $raw:expr $(,)?) => {
        if $enc.is_signed() {
            trace_field!(_impl pre $field, $enc, $enc.is_signed(), $crate::utils::as_i32($raw));
        } else {
            trace_field!(_impl pre $field, $enc, $enc.is_signed(), $raw);
        }
    };
    (post, field = $field:expr, pred = $pred:expr, final = $final:expr $(,)?) => {
        if $field.signed() {
            trace_field!(_impl post $field, $pred, $field.signed(), $crate::utils::as_i32($final));
        } else {
            trace_field!(_impl post $field, $pred, $field.signed(), $final);
        }
    };
}
