# Synapse

## Introduction

A rust based operating system using a microkernel design.
The kernel relies on satus (which is a UEFI bootloader written in rust) to load the kernel and also, 
optionally, a set of kernel modules.
The modules can take ownership of some hardware elements, and/or install themeslves into a service tree 
and provide services for applications or other modules.


## Build Environment

My build environment is Linux-based so I can't guarantee it'll build 
under anything else, although I don't intent to limit the build to 
only Linux systems.

I have the kernel and bootloader repos synced next to each other, as below:
```
code/
   satus/
     esp/
       efi/
         boot/
           modules/
   synapse/
```
Running `make boot` will compile the kernel and copy it into place 
(at efi/boot/kernel.elf of the emulated boot partition) and then 
execute the run script from the satus repo.

## To Build

Prior to building for the first time you'll need to download the core 
library source:
```
rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
```

The build will compile this to the custom build target and link to 
the kernel.

To build them both, simply use the provided Makefile:
```
make
```

## To execute tests

Tests override the custom target in order to build locally.
The Makefile explicitly encodes `x86_64-unknown-linux-gnu` so 
you may need to tweak that for your local system.

Executing the tests can then be done using the provided Makefile:
```
make test
```

# Debugging

If the kernel is run under qemu, it can be utilize qemu's remote gdb server 
in order to step through code.

In order to facilitate this, I have the following in my `~/.gdbinit`:

```
define connect-qemu
  target remote localhost:1234
end

define load-kernel
  add-symbol-file /home/jweeks/code/synapse/target/target.x86_64/debug/kernel
end

set print pretty on
set disassemble-next-line on
# if SMP is enabled in the kernel, software breakpoints don't seem to work
alias break = hbreak
```

And then to start your debug session, use the `make debug` command, which 
will start qemu up and wait for something to connect to it's remote gdb 
server.

In other terminal you can do this via:

```
gdb
(gdb) connect-qemu
(gdb) continue
```

Which will then start the bootloader running.  The bootloader intentional 
stops before loading the kernel and waits for the `esc` key to be pressed.
This is intended to be the developers opportunity to `ctrl-c` in gdb (to 
break into the gdb interactive shell) and set breakpoints before the kernel 
starts.

An example of this can be seen below:

```
^C
Thread 4 received signal SIGINT, Interrupt.
[Switching to Thread 1.4]
0x000000007f10d0d1 in ?? ()
(gdb) load-kernel 
add symbol table from file "/home/jweeks/code/synapse/target/target.x86_64/debug/kernel"
(gdb) b kernel::arch::init 
Breakpoint 1 at 0xffffff800000300e: file src/logger.rs, line 120.
(gdb) continue
```