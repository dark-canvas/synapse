# Synergy

## Introduction

A rust based microkernel designed.

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
