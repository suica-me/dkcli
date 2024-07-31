install-is-canceled = Installation has been cancelled.
another-install-is-running = Another instance of the installer is running.
install-from-config = Launching unattended installation with configuration { $path } ...
formatting-partition = Formatting partitions
downloading-system-release = Downloading system release
unpacking-system-release = Unpacking system release
generating-fstab = Generating fstab
generating-initramfs = Generating initramfs (initial RAM filesystem)
installing-bootloader = Installing and configuring GRUB bootloader
generating-ssh-key = Generating SSH Key
finalizing-installation = Finalizing installation
finished = Installation has completed successfully.
direct-efi-error = Unable to detect if your device is an EFI device.
efi-field-not-set = `efi_disk' is not set in the unattended configuration.
invaild-fullname = Invaild full name: { $e }
invaild-username = Invaild UNIX username: { $e }
invaild-hostname = Invaild hostname: { $e }
invaild-locale = Invaild locale: { $s }
invaild-timezone = Invaild timezone: { $s }
invaild-target-partition = Cannot find target partition or target partition does not have sufficient capacity.
invaild-efi-partition = The specified EFI System Partition (ESP) cannot be found.
offline-mode = Offline media detected. Would you like to install AOSC OS in offline mode?
variant = System edition
list-of-device = List of available storage devices:
no-device-to-install = There is no storage device on which AOSC OS could be installed.
no-partition-to-install = The specified storage device does not have a partition on which AOSC OS could be installed.
select-device = Storage device
auto-partiton = Would you like the installer to automatically partition your storage device?
direct-lvm-error = Unable to detect if your storage device is an LVM member.
unsupport-lvm-device = Installer does not supprt installing AOSC OS on LVM member devices.
select-system-partition = System partition
no-efi-partition = There is no available EFI System Partition (ESP) on the specified storage device.
select-efi-partition = EFI System Partition (ESP)
fullname = Full name
username = UNIX Username
hostname = Hostname
locale = System locale
timezone = Timezone
password = Password
rtc-as-localtime = Would you like to use RTC (hardware clock) as local time?
swap-size = Size of the swapfile (GiB)
hostname-illage = The specified hostname contains invalid character(s): { $c }
username-illage = The specified username contains invalid character(s): { $c }
fullname-illage = The specified full name contains invalid character: ':'
squashfs-empty = The system release manifest does not contain `squashfs' field and may be corrupted.