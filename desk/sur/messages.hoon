::  Messenger: direct messages and group conversations between ships. Each ship
::  keeps its own copy. An outgoing message is %pending until the peer's %messages
::  agent acks the poke, then %delivered or %failed. Incoming messages are %received.
::  A group message goes to every other member, and is %pending until all of them
::  have acked (waiting lists the ones that have not).
::
|%
+$  status  ?(%pending %delivered %failed %received)
::  a group's id: its owner, a dash, and the owner's count, like ~zod-3 (a dot
::  would end the id in a URL path)
+$  gid  @t
+$  message
  $:  id=@ud
      ::  the other ship, or for a group message its author (ours when we wrote it)
      peer=@p
      outgoing=?
      body=@t
      sent=@da
      =status
      ::  the group this message is in, if it is a group message
      group=(unit gid)
      ::  group sends only: members whose copy the agent has not acked yet
      waiting=(set @p)
  ==
::  the first version of a message, before group conversations
+$  message-1
  $:  id=@ud
      peer=@p
      outgoing=?
      body=@t
      sent=@da
      =status
  ==
+$  group
  $:  gid=gid
      title=@t
      owner=@p
      members=(set @p)
  ==
+$  state-0
  $:  %0
      messages=(map @ud message-1)
      next-id=@ud
      ::  highest message id the user has read, per peer
      read=(map @p @ud)
      api-key=(unit @t)
  ==
+$  state-1
  $:  %1
      messages=(map @ud message-1)
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
+$  state-2
  $:  %2
      messages=(map @ud message)
      next-id=@ud
      read=(map @p @ud)
      api-key=(unit @t)
      blocked=(set @p)
      recent=(map @p (list @da))
      ::  groups we belong to, by id
      groups=(map gid group)
      ::  the count the next group we start will take (ours, so ids do not clash)
      next-group=@ud
      ::  highest message id read, per group
      group-read=(map gid @ud)
  ==
+$  versioned-state  $%(state-0 state-1 state-2)
--
