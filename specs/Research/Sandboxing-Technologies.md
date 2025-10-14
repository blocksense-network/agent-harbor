## Context

The goal of this research is to identify potential sandboxing technologies that Agent Harbor might use in [Local-Mode](../Public/Local-Mode.md) (for product overview, see [Product-One-Pager](../../docs/Product-One-Pager.md)).

Such sandbox may limit writes to the file system outside of few specific whitelisted VSC working copies.

It may prevent reading of specific sensitive information on the system (or may require the user to maintain a list of directories which the agent should be able to read - e.g. software packages, documentation, etc).

It may prevent access to localhost services, the local network, limit the internet access to specific ports and hosts or cut it off altogether.

## Research Task

Your task is to populate this file with details about the most modern sandboxing technologies on mainstream operating systems, such as macOS, Linux and Windows and others. For each operating system, prepare a section with details.

The research should also try to identify other risks and potentially useful controls that are not mentioned above.
