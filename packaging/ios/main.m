// iPhone/iPad launcher: hands control to the game (exported by #[bevy_main]).
extern void main_rs(void);

int main(int argc, char *argv[]) {
    (void)argc;
    (void)argv;
    main_rs();
    return 0;
}
