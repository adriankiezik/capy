# Commits

Commit should use conventional commits pattern

# Modules

Each module that defines any configuration or error types has to be defined as directory module with mod.rs and those both errors and config has to be defined as seperate files. mod.rs should be kept clean with just exports. 

# Libraries

Each library module has to use `thiserror`. No .unwrap() or expect(), or any other manual handling. No library module should hardcode anything - it should be configurable for consumer of the library. They also need to follow https://rust-lang.github.io/api-guidelines/checklist.html

# Architecture

Architecture needs to precisely follow concepts from A Philosophy of Software Design by John Ousterhout
