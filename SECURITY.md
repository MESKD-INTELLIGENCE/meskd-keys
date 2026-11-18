# Security

## Reporting

Email contact@meskd.com. Please do not open a public issue for security problems.

## What the app touches

- `~/.ssh` (or the folder given with `--ssh-dir`). It reads key files to show their type, comment and fingerprint, writes new keys and imported keys there, and deletes keys when you confirm.
- `~/.ssh/config` when you add or remove a host. The file is rewritten atomically and keeps its other contents; a host block is only removed when exactly one block uses that alias.
- The clipboard, for public keys and `ssh` commands.

It does not read or write anywhere else and contains no network code.

## Passphrases

A passphrase is only used while creating a new key, to encrypt the private key before it is written. It is held in memory during that step and dropped afterwards. The app never asks for an existing key's passphrase, so it cannot decrypt or export private key material, and it never prints or copies a private key.

## Permissions

On Linux and macOS, created private keys are written with `0600`, public keys with `0644`, and the SSH config with `0600`. Keys that already exist with looser permissions are marked in the list and can be tightened from the account menu.

On Windows, files inherit the ACL of your user profile, which already excludes other users from `.ssh` by default.

## Deleting

Deleting a key removes both the private and public file. There is no undo. Confirmation is always required.
