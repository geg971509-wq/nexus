#include "macos.h"

#import <AppKit/AppKit.h>

#include <QWindow>

void hideNativeTitle(QWindow *window) {
    if (!window)
        return;
    NSView *view = (__bridge NSView *)reinterpret_cast<void *>(window->winId());
    NSWindow *nsw = view ? [view window] : nil;
    if (!nsw)
        return;
    nsw.titleVisibility = NSWindowTitleHidden;
}

