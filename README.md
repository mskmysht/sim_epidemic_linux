# SimEpidemic
Individual-based Epidemic Simulator (2020-21)

- Developed by Tatsuo Unemi, under cooperation with Saki Nawata and Masaaki Miyashita.
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
  - `list`: list existing worlds
  - `start {number}`: start the world of a given number in new thread
  - `stop {number}`: not yet implemented
  - `delete {number}`: not yet implemented
  - [for development] `debug {number}`: execute some debug process for the world of a given number
- All parameters are set to a world in which is created via `WorldParams` structure and `RuntimeParams` structure.
- You can find more detail at [http://www.intlab.soka.ac.jp/~unemi/SimEpidemic1/info/](http://www.intlab.soka.ac.jp/~unemi/SimEpidemic1/info/).

&copy; Masaaki Miyashita and Tatsuo Unemi, 2020-21, All rights reserved.
