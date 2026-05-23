#include "hal_gpio.h"
#include "app_config.h"

app_state_t g_state = {0, 0};

extern void process_data(void);

int main(void) {
    hal_gpio_init();
    g_state.active = 1;
    
    while (1) {
        process_data();
    }
    return 0;
}
