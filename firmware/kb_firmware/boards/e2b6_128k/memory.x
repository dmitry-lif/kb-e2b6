MEMORY
{
  /* E2B6 board: STM32G474CB, 128KiB flash configured SINGLE bank (DBANK=0),
     4KiB erase page. Reserve last 12KiB (3 erase pages): LED persist + Slot A + Slot B. */
  FLASH : ORIGIN = 0x08000000, LENGTH = 128K - 12K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
