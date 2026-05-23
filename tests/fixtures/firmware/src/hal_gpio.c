#include "hal_gpio.h"

static void delay_cycles(int cycles) {
    while (cycles--) {
        // asm volatile("nop");
    }
}

void hal_gpio_init(void) {
    delay_cycles(100);
    // Init logic
}

void hal_gpio_toggle(int pin) {
    // Toggle logic
}
