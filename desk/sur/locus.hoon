::  Clan command plane. Members are ships; OS-mutating
::  functions queue until a human (owner/member, never agent) approves.
::  Audit is append-only. history.purge / zfs.destroy are always denied.
::
|%
+$  role  ?(%owner %member %agent)
+$  member
  $:  who=ship
      name=@t
      =role
  ==
+$  status  ?(%pending %approved %denied %executed)
+$  job
  $:  id=@ud
      at=@da
      actor=ship
      fn=@t
      args=json
      =status
      approved-by=(unit ship)
      result=@t
  ==
+$  audit
  $:  id=@ud
      at=@da
      actor=ship
      fn=@t
      args=json
      result=@t
  ==
+$  state-0
  $:  %0
      members=(map ship member)
      jobs=(map @ud job)
      log=(list audit)
      keys=(map @t ship)
      next-id=@ud
      api-key=(unit @t)
  ==
--
