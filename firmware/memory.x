/* Adafruit MacroPad RP2040: 8 MB flash, 264 KB SRAM (256 KB striped + 2x4 KB).
   The last 128 KB of flash hold the two config slots (see src/storage.rs). */
MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 8192K - 0x100 - 128K
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}
