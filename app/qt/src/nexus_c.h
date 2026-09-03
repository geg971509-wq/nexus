#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// nexus_invoke returns an owned UTF-8 string. Release every non-null returned
// pointer exactly once with nexus_free.
char *nexus_invoke(const char *cmd, const char *json);
void nexus_free(char *ptr);
void nexus_teardown(void);
void nexus_init(void);
void nexus_set_event_cb(void (*cb)(const char *name, const char *json));
void nexus_set_tray_visible_cb(void (*cb)(bool visible));
void nexus_set_spinning_cb(void (*cb)(bool spinning));

#ifdef __cplusplus
}
#endif
