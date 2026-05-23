#ifndef HAL_GPIO_H
#define HAL_GPIO_H

typedef enum {
    GPIO_LOW = 0,
    GPIO_HIGH = 1
} gpio_state_t;

void hal_gpio_init(void);
void hal_gpio_toggle(int pin);

#endif // HAL_GPIO_H
