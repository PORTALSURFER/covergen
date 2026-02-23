# Active TODO (Ordered)

1. Add non-layer node kinds (explicit blend, mask, tone-map, warp) and validate typed wiring for them.
2. Enable true DAG execution (fan-out/fan-in), including merge semantics and deterministic scheduling.
3. Move remaining host-side finishing passes (contrast/stretch/downsample) into GPU compute passes.
4. Add direct animation video pipeline (stream to ffmpeg) to avoid large temporary frame sets.
5. Add V1 vs V2 benchmark + visual regression suite and define cutover gates for making V2 default.
