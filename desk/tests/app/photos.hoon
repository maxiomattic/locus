::  P4 photo catalog — create, list, expectedRevision conflict, reject .. paths.
::
/-  pho=photos
/+  *test-agent
/=  photos-agent  /app/photos
|%
++  dap  %photos
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap photos-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  create
  %-  pairs:enjs:format
  :~  fn+s+'photo.create'
      args+(pairs:enjs:format ~[album+s+'holidays' title+s+'pier' path+s+'holidays/pier.jpg'])
  ==
::
++  bad-path
  %-  pairs:enjs:format
  :~  fn+s+'photo.create'
      args+(pairs:enjs:format ~[album+s+'x' title+s+'x' path+s+'../etc/passwd'])
  ==
::
++  patch
  |=  [id=@ud rev=@ud title=@t]
  %-  pairs:enjs:format
  :~  fn+s+'photo.patch'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format rev) title+s+title])
  ==
::
++  test-create-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  =cage  bind:m  (got-peek /x/photos)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  (pure:m ~)
::
++  test-stale-patch-keeps-revision
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  *  bind:m  (call (patch 1 0 'ok'))
  ;<  *  bind:m  (call (patch 1 0 'stale'))
  ;<  =cage  bind:m  (got-peek /x/photos)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  ?>  ?=(%o -.i.p.jon)
  =/  rev=json  (~(got by p.i.p.jon) 'revision')
  ?>  ?=(%n -.rev)
  ?>  =('1' p.rev)
  (pure:m ~)
::
++  test-reject-dotdot-path
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call bad-path)
  ;<  =cage  bind:m  (got-peek /x/photos)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=(~ p.jon)
  (pure:m ~)
--
