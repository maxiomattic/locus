::  Household contacts (P4). Canonical people live here, not a second DB.
::
|%
+$  contact
  $:  id=@ud
      name=@t
      email=@t
      ship=@t
      revision=@ud
  ==
+$  state-0
  $:  %0
      contacts=(map @ud contact)
      next-id=@ud
      api-key=(unit @t)
  ==
--
