::  Household calendar store (P2). Canonical events live here.
::  Host CalDAV/ICS is a bridge into this map, not a second database.
::
|%
+$  event
  $:  id=@ud
      calendar=@t
      title=@t
      start=@da
      end=@da
      body=@t
      revision=@ud
  ==
+$  state-0
  $:  %0
      events=(map @ud event)
      next-id=@ud
      api-key=(unit @t)
  ==
--
