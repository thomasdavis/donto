// pgrx 0.12 schema-emit shim. Required by `cargo pgrx package` /
// `cargo pgrx schema`: it builds this binary, runs it, and captures the
// SQL descriptors compiled into the cdylib.
::pgrx::pgrx_embed!();
