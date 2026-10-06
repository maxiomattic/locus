::  Photo catalog (P4). Bytes live on ZFS; this map is titles, albums, tags, paths.
::
|%
+$  photo
  $:  id=@ud
      album=@t
      title=@t
      path=@t
      tags=(list @t)
      revision=@ud
  ==
+$  state-0
  $:  %0
      photos=(map @ud photo)
      next-id=@ud
      api-key=(unit @t)
  ==
--
