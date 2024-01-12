---
layout: post
title: Weekly Development Update - Async Rust
image: assets/BC_Logo_.png
---

I plan to post a weekly development update on this blog. I will use
the category `development-updates` for these.

In the first week of working in the Rust ecosystem I share my
learnings around aysnc rust.

## Async Rust

As part of the work to deal with network programming, I looked at
options in the Rust ecosystem. Here is what I learnt:

### The Good

1. Tokio and async-std are the two best options the ecosystem offers.
2. Crossbeam is a no-brainer to use when we need lock-free
   data structures. They even have concurrent lock-free maps.

### The Bad

1. Clearly Rust ecosystem is very new compared to what is available in
   C++. For example, it is hard to find maintained threadpool
   libraries outside the async libraries. None of the maintained
   threadpools offer priority threads or tasks. Tokio and async-std
   don't support assigning priorities to tasks.
3. Supporting long running computations requires a completely
   different runtime from the one used for async io. This stems from
   the Rust supporting stackless coroutines which requires cooperative
   scheduling between tasks to make progress.

For async io, the best options are Tokio and async std. If you need to
run long running tasks on a thread, Tokio offers a hacky solution to
use a blocking thread. We still can't specify priorities on those
threads. Tokio has introduced a solution inspired by node.js that
tracks ticks consumed by each task and as it hits its budget, it stops
making progress, giving other tasks a chance to make progress.

## Plan and Options

My plan is to stick with Tokio and use crossbeam deque to provide
priority queues. The catch is even if we schedule a task on the high
priority queue, there is no guarantee that Tokio won't just preempt it
when it awaits on some IO. If this becomes a problem, we will need to
look at building some sort of threadpool ourselves using crossbeam and
scheduling tasks by priority. Or we could use Tokio's `spawn_blocking`
call and never yield till the end.

### Links

[Async: What is blocking?](https://ryhl.io/blog/async-what-is-blocking/#the-rayon-crate)

[Reducing tail latencies with automatic cooperative task yielding](https://tokio.rs/blog/2020-04-preemption)

[ Using Rustlang’s Async Tokio Runtime for CPU-Bound Tasks ](https://thenewstack.io/using-rustlangs-async-tokio-runtime-for-cpu-bound-tasks/)

[Async Rust in Practice: Performance, Pitfalls, Profiling](https://www.scylladb.com/2022/01/12/async-rust-in-practice-performance-pitfalls-profiling/)

