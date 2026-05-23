#ifndef APP_CONFIG_H
#define APP_CONFIG_H

#define MAX_RETRIES 5

typedef struct {
    int active;
    int retry_count;
} app_state_t;

#endif // APP_CONFIG_H
