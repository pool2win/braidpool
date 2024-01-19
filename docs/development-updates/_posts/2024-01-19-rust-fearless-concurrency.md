---
layout: post
title: Weekly Development Update - Fearless Concurrency
image: assets/BC_Logo_.png
---

The main focus this week was to pick up the tricks of trade in Rust
for concurrent programming. I also put some effort in [bumping up the
test coverage to 90%](https://app.codecov.io/gh/pool2win/braidpool). I
am also tracking development as a [github project with a
roadmap](https://github.com/users/pool2win/projects/10/views/3?sortedBy%5Bdirection%5D=asc&sortedBy%5BcolumnId%5D=72720720).

## Rust Fearless Concurrency

Initially dealing with Rust's approach to concurrent programming was a
bit strange. However, once one understands how Rust wants us to access
shared memory - which is to use Atomic Reference Counted (Arc) pointer
to wrap mutexes. The surprising thing was to realise once you have an
Arc pointer you need to call `clone` on it to increment the ref
counter.

Anything left implied in C++ and left in the developer's mind has to
be made explicit in Rust. Which is great as it seems like there is a
nice safety net around me, however, I do wonder the performance
implications of using Arc, each time we need to access a Mutex.

## Mocking for tests

One annoying thing to deal with with unit tests is still dependence on
mocks. There isn't good support for stubs yet. It'll be awesome when
someone puts their mind down to building a crate to write stubs for
Rust unit tests.


## Next few weeks

With the ground work on async and concurrent programming out of the
way as well as tracking code coverage, we can now switch gears and
start working on reliable broadcast and finally FROST integration -
which is my goal for end of February.
