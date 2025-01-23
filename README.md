# Bazel Mini Container Image Tools

This repository contains the rust source code for the tools binaries used in
[`rules_minidock`](https://github.com/bazeltools/rules_minidock).

## Release process

To cut a new release, use the
[Releases](https://github.com/bazeltools/rules_minidock_tools/tags)
UI to "Draft a new release" and hit the button to generate release notes.
A new release will trigger GitHub actions to build the various binaries
and attach them to the release.

To pull a new tools release into `rules_minidock`, see that repo's
[`update_remote_tools.sh`](https://github.com/bazeltools/rules_minidock/blob/main/minidock/remote_tools/update_remote_tools.sh).
