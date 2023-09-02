# roon-tui

### A Roon Remote for the terminal

![Roon TUI screenshot](images/screenshot.png)

## Building from Source Code
* Install Rust: visit [rustup.rs](https://rustup.rs/) and follow the provided instructions
* Clone the roon-tui git repository: `git clone https://github.com/TheAppgineer/roon-tui.git`
* Change directory and build the project: `cd roon-tui && cargo build --release`
* The binary can be found in: `target/release/roon-tui`

## Downloading Release Binaries
Prebuilt binaries can be downloaded from the [latests release](https://github.com/TheAppgineer/roon-tui/releases/latest) page on GitHub. Binaries might have been created by other users for platforms I don't have access to myself.

## Using Homebrew (macOS)
User [Nepherte](https://github.com/Nepherte) created a Homebrew tap from which you can install Roon TUI. Instructions can be found at https://github.com/Nepherte/homebrew-roon.

## Authorizing Core Access
On first execution the outside border of the UI will be highlighted without any views active, this indicates that pairing with a Roon Core has to take place. Use your Roon Remote and select Settings&rarr;Extensions from the hamburger menu and then Enable Roon TUI.

## Project Status
This is Alpha stage software. Instead of using the official [Node.js Roon API](https://github.com/RoonLabs/node-roon-api) provided by Roon Labs this project uses an own developed [Rust port](https://github.com/TheAppgineer/rust-roon-api) of the API.

## Usage Instructions
### Specifying Configuration File on Command Line
At startup the default location to get the `config.json` configuration file from is the current working directory. This is troublesome when the executable is placed in a system folder and accessed by using the `PATH` environment variable, because the user account might not have permissions to write to that location. This can be solved by placing the configuration file somewehere in the home folder and specifying its location at startup on the command line. In the below example the file is stored in the users `.config` folder:

    roon-tui ~/.config/roon-tui/config.json

### Multi-character Jump in Browse View
After a list of Artists, Albums, etc. is selected, and it is known what to play, a name can be directly typed in the Browse View. The first item that matches the input will be selected. The currently matched characters are displayed in the lower left corner of the view. The Backspace key can be used to revert to previous selections, the Home keys clears the complete input.

Some important remarks:
* Relies on sort setting for Artists and Composers, type first/last name depending on setting
* Ignores "The" in item names, as this is not used in sorting, meaning "The" should not be included in the input
* Is case insensitive
* Only supports ASCII characters as input, i.e., no unicode input
* Any unicode characters in items are converted to closest ASCII match before matching takes place

## Key Bindings
### Global (useable in all views)
|||
|---|---|
|Tab|Switch between views
|Shift-Tab|Reverse switch between views
|Ctrl-z|Open zone selector
|Ctrl-p|Play / Pause
|Ctrl-e|Pause at End of Track
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
|a...z|Multi-character jump to item
|Backspace|Step back in multi-character jump
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
