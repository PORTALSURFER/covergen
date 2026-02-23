//! Deterministic visual regression tests for V2 fixed-seed renders.
//!
//! These tests snapshot hashes of CPU-only V2 graph output for still images and
//! sampled animation frames. Using CPU-only node kinds keeps results portable
//! and deterministic across hosts without requiring a hardware GPU.

use std::error::Error;

use super::cli::V2Profile;
use super::node::GraphTimeInput;
use super::runtime::finalize_luma_for_output_for_test;
use super::runtime_eval::render_graph_luma;

#[path = "visual_regression_fixtures.rs"]
mod fixtures;

#[derive(Clone, Copy, Debug)]
struct StillSnapshotCase {
    name: &'static str,
    seed: u32,
    width: u32,
    height: u32,
    profile: V2Profile,
    graph: fixtures::SnapshotGraphKind,
    expected_hash: u64,
}

#[derive(Clone, Copy, Debug)]
struct AnimationSnapshotCase {
    name: &'static str,
    seed: u32,
    width: u32,
    height: u32,
    profile: V2Profile,
    graph: fixtures::SnapshotGraphKind,
    frame_total: u32,
    frame_indices: &'static [u32],
    expected_hashes: &'static [u64],
}

const STILL_SNAPSHOTS: &[StillSnapshotCase] = &[
    StillSnapshotCase {
        name: "cpu-weave-still-192",
        seed: 0x1357_9BDF,
        width: 192,
        height: 192,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::Weave,
        expected_hash: 0x7bce_fca6_cc4c_b01c,
    },
    StillSnapshotCase {
        name: "cpu-mask-atlas-still-256",
        seed: 0x2468_ACE0,
        width: 256,
        height: 256,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::MaskAtlas,
        expected_hash: 0xc63a_e044_7cc6_abc9,
    },
    StillSnapshotCase {
        name: "cpu-warp-grid-still-384",
        seed: 0xDEAD_BEEF,
        width: 384,
        height: 384,
        profile: V2Profile::Quality,
        graph: fixtures::SnapshotGraphKind::WarpGrid,
        expected_hash: 0x9248_7525_3c39_8c01,
    },
];

const ANIMATION_WEAVE_INDICES: &[u32] = &[0, 4, 8, 12, 16, 20, 24, 31];
const ANIMATION_MASK_ATLAS_INDICES: &[u32] = &[0, 5, 10, 15, 20, 25, 29];

const ANIMATION_SNAPSHOTS: &[AnimationSnapshotCase] = &[
    AnimationSnapshotCase {
        name: "cpu-weave-animation-32f",
        seed: 0xA5A5_1F1F,
        width: 192,
        height: 192,
        profile: V2Profile::Performance,
        graph: fixtures::SnapshotGraphKind::Weave,
        frame_total: 32,
        frame_indices: ANIMATION_WEAVE_INDICES,
        expected_hashes: &[
            0xbaee_262c_d44f_b7ab,
            0x764d_05aa_427f_49bc,
            0xe086_f6dd_ac4e_37a6,
            0xadff_e066_f125_81f3,
            0x13b5_df2c_fdae_5d84,
            0x15ea_0900_555d_71e3,
            0x3562_d9c3_e732_75e3,
            0x77a2_16ea_8806_196f,
        ],
    },
    AnimationSnapshotCase {
        name: "cpu-mask-atlas-animation-30f",
        seed: 0x55AA_7788,
        width: 256,
        height: 256,
        profile: V2Profile::Quality,
        graph: fixtures::SnapshotGraphKind::MaskAtlas,
        frame_total: 30,
        frame_indices: ANIMATION_MASK_ATLAS_INDICES,
        expected_hashes: &[
            0x9c2e_d240_0666_0216,
            0x3f35_a0d5_e5df_4ada,
            0xef78_319d_7e12_442d,
            0xc372_c6d7_a772_bf93,
            0xa33e_aa03_9cbd_2a67,
            0x120d_adcc_07ec_19c8,
            0xada7_5900_384d_8617,
        ],
    },
];

#[test]
fn v2_still_fixed_seed_snapshots_match() {
    for case in STILL_SNAPSHOTS {
        let actual_hash = render_still_hash(*case)
            .unwrap_or_else(|err| panic!("snapshot '{}': {err}", case.name));
        assert_eq!(
            actual_hash, case.expected_hash,
            "snapshot '{}' drifted: expected {:#018x}, got {:#018x}",
            case.name, case.expected_hash, actual_hash
        );
    }
}

#[test]
fn v2_animation_fixed_seed_sampled_frames_match() {
    for case in ANIMATION_SNAPSHOTS {
        let actual_hashes = render_animation_hashes(*case)
            .unwrap_or_else(|err| panic!("animation snapshot '{}': {err}", case.name));
        assert_eq!(
            actual_hashes.len(),
            case.expected_hashes.len(),
            "animation snapshot '{}' has mismatched expected hash count",
            case.name
        );
        for (index, actual_hash) in actual_hashes.into_iter().enumerate() {
            let expected_hash = case.expected_hashes[index];
            assert_eq!(
                actual_hash, expected_hash,
                "animation snapshot '{}' frame {} drifted: expected {:#018x}, got {:#018x}",
                case.name, case.frame_indices[index], expected_hash, actual_hash
            );
        }
    }
}

#[test]
#[ignore = "manual snapshot refresh helper; run with --ignored --nocapture"]
fn dump_visual_snapshot_hashes() {
    for case in STILL_SNAPSHOTS {
        let hash = render_still_hash(*case).expect("still snapshot hash");
        eprintln!("still {:<28} => {:#018x}", case.name, hash);
    }

    for case in ANIMATION_SNAPSHOTS {
        let hashes = render_animation_hashes(*case).expect("animation snapshot hashes");
        eprintln!("animation {:<24}", case.name);
        for (idx, hash) in hashes.into_iter().enumerate() {
            eprintln!("  frame {:>3} => {:#018x}", case.frame_indices[idx], hash);
        }
    }
}

fn render_still_hash(case: StillSnapshotCase) -> Result<u64, Box<dyn Error>> {
    let config = fixtures::snapshot_config(case.seed, case.width, case.height, case.profile);
    let compiled =
        fixtures::build_cpu_only_compiled(case.seed, config.width, config.height, case.graph)?;
    let mut buffers = fixtures::runtime_buffers(&config, &compiled)?;

    render_graph_luma(
        &compiled,
        None,
        &mut buffers,
        config.seed.wrapping_add(compiled.seed),
        None,
    )?;
    finalize_luma_for_output_for_test(&config, &compiled, None, &mut buffers)?;
    Ok(fixtures::fnv1a64(&buffers.output_gray))
}

fn render_animation_hashes(case: AnimationSnapshotCase) -> Result<Vec<u64>, Box<dyn Error>> {
    let config = fixtures::snapshot_config(case.seed, case.width, case.height, case.profile);
    let compiled =
        fixtures::build_cpu_only_compiled(case.seed, config.width, config.height, case.graph)?;
    let mut buffers = fixtures::runtime_buffers(&config, &compiled)?;
    let mut hashes = Vec::with_capacity(case.frame_indices.len());

    for &frame_index in case.frame_indices {
        if frame_index >= case.frame_total {
            return Err(format!(
                "invalid frame index {} for total frame count {}",
                frame_index, case.frame_total
            )
            .into());
        }

        let graph_time = GraphTimeInput::from_frame(frame_index, case.frame_total);
        let seed_offset = config
            .seed
            .wrapping_add(compiled.seed)
            .wrapping_add(frame_index.wrapping_mul(0x9E37_79B9));

        render_graph_luma(&compiled, None, &mut buffers, seed_offset, Some(graph_time))?;
        finalize_luma_for_output_for_test(&config, &compiled, None, &mut buffers)?;
        hashes.push(fixtures::fnv1a64(&buffers.output_gray));
    }

    Ok(hashes)
}
