#![doc = include_str!("../readme.md")]
#![no_std]

// Slyced: copy of windows-link 0.2.1 (MIT OR Apache-2.0, microsoft/windows-rs).
// The only change is for the Windows 7 targets (`target_vendor = "win7"`):
// combase.dll does not exist there, so imports from it are taken from
// ole32.dll, which has the same COM functions on Windows 7 (CoTaskMemFree...).
// Every other target links exactly as the original crate.

/// Defines an external function to import.
#[cfg(all(windows, target_arch = "x86", not(target_vendor = "win7")))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import.
#[cfg(all(windows, not(target_arch = "x86"), not(target_vendor = "win7")))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import (Windows 7, 32 bits).
#[cfg(all(windows, target_arch = "x86", target_vendor = "win7"))]
#[macro_export]
macro_rules! link {
    ("combase.dll" $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = "ole32.dll", kind = "raw-dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    );
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim", import_name_type = "undecorated")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import (Windows 7, 64 bits).
#[cfg(all(windows, not(target_arch = "x86"), target_vendor = "win7"))]
#[macro_export]
macro_rules! link {
    ("combase.dll" $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = "ole32.dll", kind = "raw-dylib", modifiers = "+verbatim")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    );
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        #[link(name = $library, kind = "raw-dylib", modifiers = "+verbatim")]
        extern $abi {
            $(#[link_name=$link_name])?
            pub fn $($function)*;
        }
    )
}

/// Defines an external function to import.
#[cfg(not(windows))]
#[macro_export]
macro_rules! link {
    ($library:literal $abi:literal $($link_name:literal)? fn $($function:tt)*) => (
        extern $abi {
            pub fn $($function)*;
        }
    )
}
