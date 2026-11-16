# dirsize
Determine directory size in parallel

MSRV: 1.63.0

## Usage

```
dirsize [DIRECTORY]
```

Lists each directory directly under `DIRECTORY` (default `.`) with its total
size, largest first, followed by a total for `DIRECTORY` itself:

```
$ dirsize ~/src/project
   1.2 GiB  /Users/me/src/project/target
  48.3 KiB  /Users/me/src/project/src
   1.3 GiB  total
```

Sizes are apparent sizes (the sum of file lengths), not disk usage as reported
by `du`, so sparse files and hard links may be counted differently. Symbolic
links are counted by their own size and are never followed, except for
`DIRECTORY` itself. Entries that cannot be read are reported on stderr and
skipped; the exit status is then 1.
