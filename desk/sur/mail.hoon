::  Household mail store (P2). Canonical messages live here.
::  Host SMTP/IMAP is a bridge into this map, not a second database.
::
|%
+$  folder  ?(%inbox %sent %drafts %trash)
+$  message
  $:  id=@ud
      =folder
      from=@t
      to=(list @t)
      subject=@t
      body=@t
      at=@da
      revision=@ud
  ==
+$  state-0
  $:  %0
      messages=(map @ud message)
      next-id=@ud
      api-key=(unit @t)
  ==
--
