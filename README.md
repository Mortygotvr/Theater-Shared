# Theater Shared

The Rust library that [Theater Scene](https://github.com/Mortygotvr/Theater-Scene) and [Theater Reader](https://github.com/Mortygotvr/Theater-Reader) both build in:

- **`platform`:** the chat platforms (Twitch, Kick, YouTube) and their events.
- **`triggers`:** Theater Reader's trigger patterns (`Twitch * redeem Hydrate`): building them from lists, reading them back, and reading an alert's trigger.
- **`seen`:** the channels and redeem titles alerts came from, offered in the trigger lists.
- **`builder`:** those lists drawn with egui (the Studio's redeem editor, the Chat & Alerts tester).

Both plugins expect it next to them (`../Theater-Shared`).

`cargo test` runs its tests.

## License

GPL-2.0-or-later (see [LICENSE](LICENSE)).
