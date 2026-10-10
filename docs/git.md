# Git Guidelines

## Commit messages

Commit messages should follow the generally accepted/established guidlines (see Commit Guidlines in 
[git-scm.com](https://git-scm.com/book/ms/v2/Distributed-Git-Contributing-to-a-Project)), but with some expanded limits:
 - Headlines should be in the form `module: summary` (don't capitalize the module)
 - Headlines should stay under 80 characters (the 50 character limmit is restrictive)
 - Commit body should stay under 80 characters where possible
 - The "80 character limit" is merely to conform to existing tool expectations and allow for commits to appear nicely in side panels, etc.  Realistically, no one uses 80 character terminals anymore, and if a commit message conveys information better by using more characters per line, then use more characters per line.  The priority should be in describing the change, not fitting into a legacy terminal.

## Modules

Describe the *area* of code being modified.  Note that many of these modules describe an area of code that exists as both a portable layer across all platforms (usually defining interfaces), and platform-specific implementations.  The latter changes should also include the platform, and use the code module as a [submodule](#sub-modules).

 - **build** - Any build related commits (CI actions fall into this category as well).
 - **core** - Anything that is used throughout the entire kernel (eg. error codes, types).
 - **docs** - Documentation.
 - **init** - Basic kernel initialization.  There is likely a more specific module that can be used in most case.
 - **pager** - Page-based memory management.
 - **page-based** - The page-based utilities/primitives (eg. list, queue, stack).
 - **scheduler** - Cooperative or pre-emtive scheduling code and algorithms, task management.
 - **smp** - Configuration/management of multiple CPUs/cores.
 - **sync** - CPU/task synchronization primitives (eg. mutex, semaphore).
 - **x86-64** - Code specific to the x86-64 platform.  This is likely too generic, and a [submodule](#sub-modules) should be included.

## Sub Modules

Submodules exist to further clarify a wide-reaching module.

### X86-64 submodules

The following are examples and are not exhaustive.  Many of the modules above could also be considered a submodule of any given supported platform.

 - **x86-64/idt** - Code implementing/affecting the interrupt descriptor table.
 - **x86-64/gdt** - Code implementing/affecting the global descriptor table.
 - **x86-64/smp** - An x86-64 specific category for smp work.
 - **x86-64/timers** - Any time-based code (could be using the timer interrupt, hpet, tsc, etc).
