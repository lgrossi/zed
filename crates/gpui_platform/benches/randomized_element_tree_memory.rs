//! Heap memory of a seeded element tree's window, counted by [`dhat`] as the process's
//! global allocator. Run it on two GPUI revisions to compare what rendering keeps and
//! allocates for the same trees as the `randomized_element_tree` timing benchmark:
//!
//! ```text
//! cargo bench -p gpui_platform --features bench-support --bench randomized_element_tree_memory [-- FILTER]
//! ```
//!
//! For every input it opens a window on the tree and reports, relative to the heap just
//! before the window opened:
//!
//! - `held`: live bytes once the tree has drawn a few full frames: what the window
//!   keeps between frames.
//! - `peak`: the most live bytes at any point while the window was open.
//! - `growth`: live bytes gained over further full-refresh frames. Steady rendering
//!   should keep this near zero.
//! - allocations and bytes allocated per full-refresh frame and per frame in which one
//!   element changes: the churn each frame puts through the allocator.
//! - `closed`: live bytes left once the window is closed and the app has settled.
//!
//! GPU memory (Metal buffers, textures) is allocated by the driver, not the heap, and
//! is not counted. Timing is meaningless under dhat, which records a backtrace per
//! allocation; time frames with the timing benchmark instead.

use gpui::{
    BenchAppContext, BenchWindowContext, Entity,
    randomized_element_tree::{ChangeLocality, RandomizedElementTree},
};

#[path = "randomized_element_tree/tree_inputs.rs"]
mod tree_inputs;

use tree_inputs::{TreeInput, inputs};

#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

/// Full frames drawn before `held` is read, so caches that fill on the first frames
/// (text layout, the view tree's retained output) are counted as held, not as growth.
const WARM_UP_FRAMES: usize = 5;

/// Frames over which growth and per-frame allocations are counted.
const MEASURED_FRAMES: usize = 50;

struct MemoryReport {
    held: i64,
    peak: i64,
    growth: i64,
    refresh: FrameAllocations,
    one_change: FrameAllocations,
    closed: i64,
}

struct FrameAllocations {
    blocks: f64,
    bytes: f64,
}

fn main() {
    // Cargo passes `--bench` to benchmark binaries; the first other argument filters
    // inputs by name, like Criterion's.
    let filter = std::env::args()
        .skip(1)
        .find(|argument| !argument.starts_with("--"));
    println!(
        "{:<40} {:>10} {:>10} {:>9} {:>14} {:>14} {:>14} {:>14} {:>9}",
        "input",
        "held KB",
        "peak KB",
        "growth KB",
        "refresh allocs",
        "refresh KB",
        "change allocs",
        "change KB",
        "closed KB"
    );
    for input in inputs() {
        let name = input.to_string();
        if filter.as_ref().is_some_and(|filter| !name.contains(filter)) {
            continue;
        }
        let report = measure(&input);
        println!(
            "{:<40} {:>10.1} {:>10.1} {:>9.1} {:>14.0} {:>14.1} {:>14.0} {:>14.1} {:>9.1}",
            name,
            kilobytes(report.held),
            kilobytes(report.peak),
            kilobytes(report.growth),
            report.refresh.blocks,
            report.refresh.bytes / 1024.,
            report.one_change.blocks,
            report.one_change.bytes / 1024.,
            kilobytes(report.closed)
        );
    }
}

fn kilobytes(bytes: i64) -> f64 {
    bytes as f64 / 1024.
}

fn measure(input: &TreeInput) -> MemoryReport {
    let mut cx = BenchAppContext::unmeasured(
        gpui::bench_platform(
            Some(Box::new(gpui_platform::current_headless_renderer)),
            gpui_platform::current_platform(true).text_system(),
        ),
        Some("randomized_element_tree_memory"),
    );
    cx.settle();

    // dhat counts only what is allocated while its profiler runs, so everything counted
    // below was allocated for this input's window.
    let profiler = dhat::Profiler::builder().testing().build();
    let start = dhat::HeapStats::get();
    let live_since_start =
        |stats: &dhat::HeapStats| stats.curr_bytes as i64 - start.curr_bytes as i64;

    let config = input.config;
    let mut window = cx.add_empty_window();
    let tree = window.update(|window, cx| {
        window.replace_root(cx, |_, cx| {
            RandomizedElementTree::new_with_config(config, cx)
        })
    });
    window.run_until_idle();

    for _ in 0..WARM_UP_FRAMES {
        draw_frame(&mut window, &tree, |window, _, _| window.refresh());
    }
    let warm = dhat::HeapStats::get();

    for _ in 0..MEASURED_FRAMES {
        draw_frame(&mut window, &tree, |window, _, _| window.refresh());
    }
    let refreshed = dhat::HeapStats::get();

    for _ in 0..MEASURED_FRAMES {
        draw_frame(&mut window, &tree, |_, tree, cx| {
            tree.update(cx, |tree, cx| {
                tree.recolor_children(1, ChangeLocality::Localized, cx);
            });
        });
    }
    let changed = dhat::HeapStats::get();

    window.update(|window, _| window.remove_window());
    drop(tree);
    drop(window);
    cx.settle();
    let closed = dhat::HeapStats::get();
    drop(profiler);
    cx.teardown();

    MemoryReport {
        held: live_since_start(&warm),
        peak: closed.max_bytes as i64 - start.curr_bytes as i64,
        growth: live_since_start(&refreshed) - live_since_start(&warm),
        refresh: per_frame(&warm, &refreshed),
        one_change: per_frame(&refreshed, &changed),
        closed: live_since_start(&closed),
    }
}

/// Applies `update` and draws the frame it causes, asserting that the tree rendered, as
/// the timing benchmark's measured loop does.
fn draw_frame(
    window: &mut BenchWindowContext,
    tree: &Entity<RandomizedElementTree>,
    update: impl FnOnce(&mut gpui::Window, &Entity<RandomizedElementTree>, &mut gpui::App),
) {
    window.update(|window, cx| {
        tree.update(cx, |tree, _| tree.reset_work_counters());
        update(window, tree, cx);
    });
    window.run_until_idle();
    let counters = window.update(|_, cx| tree.read(cx).work_counters());
    assert!(
        counters.element_render_count() >= 1,
        "every measured frame renders the tree"
    );
}

fn per_frame(before: &dhat::HeapStats, after: &dhat::HeapStats) -> FrameAllocations {
    FrameAllocations {
        blocks: (after.total_blocks - before.total_blocks) as f64 / MEASURED_FRAMES as f64,
        bytes: (after.total_bytes - before.total_bytes) as f64 / MEASURED_FRAMES as f64,
    }
}
