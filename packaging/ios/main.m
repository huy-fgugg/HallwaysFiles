// iPhone/iPad launcher: hands control to the game (main_rs in mobile/src/lib.rs).
extern void main_rs(void);

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    main_rs();
    return 0;
}
