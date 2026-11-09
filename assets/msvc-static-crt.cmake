# CMake toolchain used for C/C++ dependencies built with CMake (CharLS, for
# JPEG-LS): link the C/C++ runtime statically (/MT), as Rust does with
# crt-static, so Slyced.exe needs no Visual C++ Redistributable (MSVCP140.dll).
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded")
