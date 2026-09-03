#include "macos.h"

#import <AppKit/AppKit.h>
#import <objc/runtime.h>

#include <QWindow>

@interface NexusSidebarVisualEffectView : NSVisualEffectView
@end

@implementation NexusSidebarVisualEffectView
- (NSView *)hitTest:(NSPoint)point {
    return nil;
}
@end

static NSWindow *nativeWindow(QWindow *window, NSView **nativeViewOut = nullptr) {
    if (!window)
        return nil;
    NSView *view = (__bridge NSView *)reinterpret_cast<void *>(window->winId());
    NSWindow *nsw = view ? [view window] : nil;
    if (nativeViewOut)
        *nativeViewOut = view;
    return nsw;
}

static void installSidebarMaterial(NSWindow *window, NSView *nativeView) {
    static char materialAssociationKey;
    if (!window || !nativeView || objc_getAssociatedObject(window, &materialAssociationKey))
        return;

    NSView *containerView = nativeView.superview;
    if (!containerView)
        return;

    window.opaque = NO;
    window.backgroundColor = NSColor.clearColor;

    NexusSidebarVisualEffectView *effectView =
        [[NexusSidebarVisualEffectView alloc] initWithFrame:nativeView.frame];
    effectView.material = NSVisualEffectMaterialSidebar;
    effectView.blendingMode = NSVisualEffectBlendingModeBehindWindow;
    effectView.state = NSVisualEffectStateFollowsWindowActiveState;
    effectView.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    // Match iDescriptor: the native material spans Qt's view. QML keeps the
    // sidebar transparent and paints the workspace with windowBackgroundMacOS,
    // so sidebar collapse/expand does not require synchronizing a native frame.
    [containerView addSubview:effectView positioned:NSWindowBelow relativeTo:nativeView];
    objc_setAssociatedObject(window, &materialAssociationKey, effectView,
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

static void setTrafficLightInset(NSWindow *window) {
    NSButton *closeButton = [window standardWindowButton:NSWindowCloseButton];
    NSButton *miniButton = [window standardWindowButton:NSWindowMiniaturizeButton];
    NSButton *zoomButton = [window standardWindowButton:NSWindowZoomButton];
    if (!closeButton || !miniButton || !zoomButton)
        return;

    NSView *titlebarContainer = closeButton.superview.superview;
    if (!titlebarContainer)
        return;

    constexpr CGFloat inset = 20.0;
    NSRect closeRect = closeButton.frame;
    const CGFloat titlebarHeight = closeRect.size.height + inset;
    NSRect titlebarRect = titlebarContainer.frame;
    titlebarRect.size.height = titlebarHeight;
    titlebarRect.origin.y = window.frame.size.height - titlebarHeight;
    titlebarContainer.frame = titlebarRect;

    const CGFloat spacing = NSMinX(miniButton.frame) - NSMinX(closeRect);
    NSArray<NSButton *> *buttons = @[closeButton, miniButton, zoomButton];
    for (NSUInteger i = 0; i < buttons.count; ++i) {
        NSButton *button = buttons[i];
        NSPoint origin = button.frame.origin;
        origin.x = inset + i * spacing;
        [button setFrameOrigin:origin];
    }
}

void styleMacosMainWindow(QWindow *window) {
    NSView *nativeView = nil;
    NSWindow *nsw = nativeWindow(window, &nativeView);
    if (!nsw)
        return;

    nsw.styleMask |= NSWindowStyleMaskFullSizeContentView;
    nsw.titleVisibility = NSWindowTitleHidden;
    nsw.titlebarAppearsTransparent = YES;
    nsw.movableByWindowBackground = NO;
    installSidebarMaterial(nsw, nativeView);
    setTrafficLightInset(nsw);
}
