//! Composing the whole command surface into one runnable application.
//!
//! The module map assigns this leaf the assembly: which invocation reaches
//! which command, where the daemon connection comes from, and how an outcome
//! becomes an exit. Assembly lives apart from the pieces it assembles so that
//! changing what a command does never means editing the thing that runs it.
