# roon-tui

A Roon Remote for the terminal

![Roon TUI screenshot](images/screenshot.png)

## Building from Source Code
* Install Rust: visit [rustup.rs](https://rustup.rs/) and follow the provided instructions
* Clone the roon-tui git repository: `git clone https://github.com/TheAppgineer/roon-tui.git`
* Change directory and build the project: `cd roon-tui && cargo build --release`
* The binary can be found in: `target/release/roon-tui`

## Downloading Release Binaries
Downloadable binaries will be provided later via the release assets in GitHub. Binaries might have to be created by other users for platforms I don't have access to myself.

## Project Status
This is Alpha stage software. Instead of using the official [Node.js Roon API](https://github.com/RoonLabs/node-roon-api) provided by Roon Labs this project uses an own developed [Rust port](https://github.com/TheAppgineer/rust-roon-api) of the API.

## Key Bindings
### Global (useable in all views)
|||
|---|---|
|Tab|Swith between views
|Ctrl-z|Open zone selector
|Ctrl-p|Play / Pause
|Ctrl-c|Quit
### Common list controls
|||
|---|---|
|&uarr;|Move up
|&darr;|Move down
|Home|Move to top
|End|Move to bottom
|Page Up|Move page up
|Page Down|Move page down
### Browse View
|||
|---|---|
|Enter|Select
|Esc|Move level up
|Ctrl-Home|Move to top level
|F5|Refresh
### Queue View
|||
|---|---|
|Enter|Play from here
### Now Playing View
|||
|---|---|
|m|Mute
|u|Unmute
|+|Increase volume
|-|Decrease volume
### Search Popup
|||
|---|---|
|Enter|Search provided term
|Esc|Back to Browse view
### Zone Select Popup
|||
|---|---|
|Enter|Select Zone
|Esc|Back to previous view
