use super::*;

pub(super) fn ctl_lfo_params() -> Vec<NodeParamSlot> {
    vec![
        param(
            param_schema::ctl_lfo::RATE_HZ,
            "rate_hz",
            0.4,
            0.0,
            8.0,
            0.05,
        ),
        param(
            param_schema::ctl_lfo::AMPLITUDE,
            "amplitude",
            0.5,
            0.0,
            64.0,
            0.1,
        ),
        param(param_schema::ctl_lfo::PHASE, "phase", 0.0, -1.0, 1.0, 0.02),
        param(param_schema::ctl_lfo::BIAS, "bias", 0.5, -1.0, 1.0, 0.02),
        param_dropdown(
            param_schema::ctl_lfo::SYNC_MODE,
            "sync_mode",
            0,
            &LFO_SYNC_MODE_OPTIONS,
        ),
        param(
            param_schema::ctl_lfo::BEAT_MUL,
            "beat_mul",
            1.0,
            0.125,
            32.0,
            0.125,
        ),
        param_dropdown(
            param_schema::ctl_lfo::LFO_TYPE,
            "type",
            0,
            &LFO_TYPE_OPTIONS,
        ),
        param(param_schema::ctl_lfo::SHAPE, "shape", 0.0, -1.0, 1.0, 0.02),
    ]
}
