# cancel — Request cancellation of a delegated scan or fetch on the local server

cancel requests a stop for a running scan or fetch delegated to the local Unix server. Copy the task ID printed by the original CLI and use the same account and data directory. A task ID identifies work, not an album or file.

The request returns immediately; the original CLI exits with code 130 once stopped. Already-saved fetch answers remain. Disconnecting the CLI or pressing its Ctrl-C does not cancel a delegated task. With no server, the command refuses; a locally running command can instead be stopped with Ctrl-C.

Completed tasks, tasks from an earlier server process and delegated operations other than scan/fetch cannot be cancelled this way. Administrative HTTP jobs use their authenticated HTTP cancel route, not this command. Local cancellation/delegation is unavailable on Windows.

## Syntax and arguments

```text
aede cancel <task-id>
```

Exactly one task ID printed by a delegated scan/fetch. Replace TASK_ID; it is a placeholder.

## Options for this command

This command has no command-specific options.

The shared [options reference](options.md) explains `--data`, `--no-color`, `--help`/`-h`, `--version`/`-v`/`-V`, option values and output/pagination rules. These shared presentation/data options do not make every command support CSV/JSON or pagination.

## Examples

```sh
aede cancel TASK_ID --data "/path/to/aede-data"
```

Uppercase ID tokens are placeholders. Copy the real identifier from Aède’s output; do not type the placeholder or angle brackets.

## Result and errors

Output describes the selected operation or catalog data. Read any per-item warnings and retained-work summary; a completed process is not an independent integrity or audio-quality guarantee. Invalid command syntax/options normally exit with code 2; handler failures normally exit with code 1. Local interruption differs from a delegated server task, as explained in [cancel](cancel.md).

## Related reading

[serve](serve.md), [scan](scan.md), [fetch](fetch.md).

Detailed existing guide: [operating.md](../operating.md).
