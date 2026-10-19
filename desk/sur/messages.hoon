::  Messenger: direct messages between ships. Each ship keeps its own copy.
::  An outgoing message is %pending until the peer's %messages agent acks the
::  poke, then %delivered or %failed. Incoming messages are %received.
::
|%
+$  status  ?(%pending %delivered %failed %received)
+$  message
  $:  id=@ud
      peer=@p
      outgoing=?
      body=@t
      sent=@da
      =status
  ==
+$  state-0
  $:  %0
      messages=(map @ud message)
      next-id=@ud
      ::  highest message id the user has read, per peer
      read=(map @p @ud)
      api-key=(unit @t)
  ==
+$  state-1
  $:  %1
      messages=(map @ud message)
      next-id=@ud
      ::  highest message id the user has read, per peer
      read=(map @p @ud)
      api-key=(unit @t)
      ::  ships whose incoming messages are refused (our sends to them still go)
      blocked=(set @p)
      ::  when each peer's recently accepted messages arrived, newest first,
      ::  for the rate limit
      recent=(map @p (list @da))
  ==
+$  versioned-state  $%(state-0 state-1)
--
