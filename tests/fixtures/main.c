#include <stdio.h>

void print_result(int value) {
    printf("Result: %d\n", value);
}

int main(void) {
    int x = add(3, 4);
    int y = subtract(10, 3);
    int z = multiply(2, 5);
    print_result(x);
    print_result(y);
    print_result(z);
    return 0;
}
