#include "include/sys/AutoRun.hpp"

// Foundation → CoreServices → CarbonCore/Script.h still carries deprecated typedefs.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#import <Foundation/Foundation.h>
#import <ServiceManagement/ServiceManagement.h>
#pragma clang diagnostic pop

// SMAppService is the supported Login Item API on modern macOS.
// Deployment target is macOS 26+, so no LSSharedFileList fallback.

void AutoRun_SetEnabled(bool enable) {
    NSError *error = nil;
    SMAppService *service = [SMAppService mainAppService];
    if (enable) {
        if (![service registerAndReturnError:&error] && error) {
            NSLog(@"Throne AutoRun enable failed: %@", error.localizedDescription);
        }
    } else {
        if (![service unregisterAndReturnError:&error] && error) {
            NSLog(@"Throne AutoRun disable failed: %@", error.localizedDescription);
        }
    }
    // Caller (mainwindow menu) re-reads AutoRun_IsEnabled() and warns on mismatch.
}

bool AutoRun_IsEnabled() {
    return [SMAppService mainAppService].status == SMAppServiceStatusEnabled;
}

void AutoRun_FixPrivilegeIfNeeded() {}

void AutoRun_MigrateIfNeeded() {}
