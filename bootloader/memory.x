MEMORY
{
  /* NOTE 1 K = 1 KiBi = 1024 bytes */
  FLASH                             : ORIGIN = 0x08000000, LENGTH = 24K
  BOOTLOADER_STATE                  : ORIGIN = 0x08006000, LENGTH = 4K
  ACTIVE                            : ORIGIN = 0x08008000, LENGTH = 256K
  APP_STATE                         : ORIGIN = 0x08047000, LENGTH = 8K
  DFU                               : ORIGIN = 0x08010000, LENGTH = 260K
  RAM                         (rwx) : ORIGIN = 0x20000000, LENGTH = 96K
}

__bootloader_state_start = ORIGIN(BOOTLOADER_STATE) - ORIGIN(FLASH);
__bootloader_state_end = ORIGIN(BOOTLOADER_STATE) + LENGTH(BOOTLOADER_STATE) - ORIGIN(FLASH);

__bootloader_active_start = ORIGIN(ACTIVE) - ORIGIN(FLASH);
__bootloader_active_end = ORIGIN(ACTIVE) + LENGTH(ACTIVE) - ORIGIN(FLASH);

__bootloader_dfu_start = ORIGIN(DFU) - ORIGIN(APP_STATE);
__bootloader_dfu_end = ORIGIN(DFU) + LENGTH(DFU) - ORIGIN(APP_STATE);