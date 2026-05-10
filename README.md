# Trippy Ants

A visually attractive simulation based on cellular automata and particle systems.

Heavily inspired by [Sebastian Lague's Slime Simulation](https://github.com/SebLague/Slime-Simulation).

![screenshot of the running simulation](screenshot.png)

The current state is a proof of concept and needs a lot of cleanup, but I promised to upload it in a timely manner. I might add the ability to live-edit parameters through a simple browser interface.

## How to run

- check out this repository to your local machine
- [install a rust toolchain](https://rust-lang.org/learn/get-started/)
- run `cargo run --release` from the terminal

Close the window using the `ESC` key or save a screenshot using the `SPACE` key.

## About AI

I used Composer 2 to get a first draft (doom fire simulation) to scaffold a window with an update loop to modify the frame-buffer.

It also generated the code for saving a PNG screenshot on pressing space.

I fully understand the entire code and the core algorithm was entirely developed by myself. I used an AI to assist with auto-complete to focus on the actual algorithm.
