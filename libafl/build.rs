fn main() {
    #[cfg(target_os = "windows")]
    windows::build!(
        windows::win32::system_services::HANDLE,
        windows::win32::windows_programming::CloseHandle,
        // API needed for the shared memory
        windows::win32::system_services::{CreateFileMappingA, OpenFileMappingA, MapViewOfFile, UnmapViewOfFile},
    );
}