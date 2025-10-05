use std::env;
use grux::grux_file_util::*;

#[test]
fn test_full_path_is_unchanged() {
    let cwd = env::current_dir().unwrap();
    let abs = cwd.join("foo/bar.txt");
    let abs_str = abs.to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&abs_str).unwrap();
    assert_eq!(result, abs_str);
}

#[test]
fn test_relative_path_is_expanded() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo/bar.txt";
    let expected = cwd.join(rel).to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(windows))]
fn test_dot_and_dotdot_are_cleaned() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo/./bar/../baz.txt";
    let expected = cwd.join("foo/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_duplicate_slashes() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo//bar///baz.txt";
    let expected = cwd.join("foo/bar/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_windows_path() {
    // Simulate a Windows-style path on any platform
    let cwd = env::current_dir().unwrap();
    let rel = "foo\\bar\\baz.txt";
    let expected = cwd.join("foo/bar/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_absolute_windows_path_cross_platform() {
    // This test ensures Windows-style absolute paths are normalized on all platforms
    let win_abs = "C:\\foo\\bar.txt";
    let expected = "C:/foo/bar.txt";
    let result = get_full_file_path(&win_abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[cfg(windows)]
#[test]
fn test_absolute_windows_path_native() {
    // Only run this test on Windows for platform-specific normalization
    let win_abs = "C:\\foo\\bar.txt";
    let expected = "C:/foo/bar.txt";
    let result = get_full_file_path(&win_abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(windows))]
fn test_absolute_linux_path() {
    let abs = "/tmp/foo/bar.txt";
    let expected = "/tmp/foo/bar.txt";
    let result = get_full_file_path(&abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(windows))]
fn test_split_path_unix_path() {
    let (dir, file) = split_path("/path1/path2", "/path1/path2/index.php");
    assert_eq!(dir, "/path1/path2");
    assert_eq!(file, "index.php");
}

#[test]
fn test_split_path_windows_path() {
    let (dir, file) = split_path(r"", r"C:\path1\path2\index.php");
    assert_eq!(dir, "C:/path1/path2");
    assert_eq!(file, "index.php");
}

#[test]
fn test_split_path_root_file() {
    let (dir, file) = split_path(r"", r"C:\file.txt");
    assert_eq!(dir, "C:/");
    assert_eq!(file, "file.txt");
}