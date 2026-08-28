//! Spin flag for the Qt tray. Frames are painted in `app/qt/src/tray.cpp`.

pub fn set_spinning(on: bool) {
    crate::qt_api::notify_spinning(on);
}
