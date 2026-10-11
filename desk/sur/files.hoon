::  File catalog. Bytes live on ZFS behind a host gateway, addressed by SHA-256.
::  Canonical entries live here; folders are implied by entries plus explicit dirs.
::
|%
+$  file
  $:  id=@ud
      dir=@t
      name=@t
      sha256=@t
      size=@ud
      mime=@t
      created=@da
      modified=@da
      revision=@ud
  ==
+$  state-0
  $:  %0
      files=(map @ud file)
      dirs=(set @t)
      next-id=@ud
      api-key=(unit @t)
  ==
--
