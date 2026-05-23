#include "hal_gpio.h"
#include "app_config.h"

extern app_state_t g_state;

static void internal_process(void) {
    if (g_state.retry_count < MAX_RETRIES) {
        hal_gpio_toggle(1);
        g_state.retry_count++;
    }
}

void process_data(void) {
    if (g_state.active) {
        internal_process();
    }
}
