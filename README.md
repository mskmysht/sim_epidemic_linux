# SimEpidemic for Linux
Individual-based Epidemic Simulator (2020-21)

- As a part of a [project](http://www.intlab.soka.ac.jp/~unemi/SimEpidemic1/info/) by Tatsuo Unemi, under cooperation with Saki Nawata and Masaaki Miyashita.
- Supported by Cabinet Secretariat of Japanese Government.

This is an individual-based simulator to help understanding the dynamics of epidemic, spread of infectous disease, mainly targetting SARS-CoV-2.

This repository includes a program rewritten in Rust which is designed to run the HTTP server version application of [SimEpidemic](https://github.com/unemi/SimEpidemic) on Linux. This project has source codes only. You need to build this project using Cargo, the Rust package manager.

## About
### Specification
- CUI application based on `App-1.7-+-Server-1.2` branch of SimEpidemic
- Features
   - Multiple worlds creation/execution with CUI commands
- Not yet implemented
   - Custom senarios
   - Data transmission via HTTP network
### Usage
- Available CUI Commands:
  - `new {number}`: create new world with a number
  - `start {world number} {the day to stop at}`: start a world to stop at the day in new thread
  - `step {world number}`: step a world
  - `stop {world number}`: stop a world
  - `reset {world number}`: reset the state of a world
  - `export {world number} {file path}`: export the statistic history of a world to a csv file
  - `delete {world number}`: delete a world
  - `list`: list all existing worlds
  - `:q`: quit this application
  - [for development] `debug {world number}`: execute some debug process for a world
- All parameters are set to a world in which is created via `WorldParams` structure and `RuntimeParams` structure.
- You can find more detail at http://www.intlab.soka.ac.jp/~unemi/SimEpidemic1/info/simepidemic-docs.html.

&copy; Masaaki Miyashita and Tatsuo Unemi, 2020-21, All rights reserved.
