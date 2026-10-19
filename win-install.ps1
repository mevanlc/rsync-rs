vcvars 
$env:CMAKE = "C:\Program Files\CMake\bin\cmake.exe"
$env:CFLAGS = "-DNDEBUG"
cargo install --path . --locked
