::  File catalog — create, list, expectedRevision conflict, reject bad dirs.
::
/-  fil=files
/+  *test-agent
/=  files-agent  /app/files
|%
++  dap  %files
++  sha
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
++  init
  =/  m  (mare ,~)
  ^-  form:m
  ;<  *  bind:m  (do-init dap files-agent)
  (pure:m ~)
::
++  call
  |=  jon=json
  (do-poke %json !>(jon))
::
++  create
  %-  pairs:enjs:format
  :~  fn+s+'file.create'
      args+(pairs:enjs:format ~[dir+s+'/school/cs101' name+s+'notes.pdf' sha256+s+sha size+(numb:enjs:format 0)])
  ==
::
++  bad-dir
  %-  pairs:enjs:format
  :~  fn+s+'file.create'
      args+(pairs:enjs:format ~[dir+s+'/a/../b' name+s+'x' sha256+s+sha size+(numb:enjs:format 0)])
  ==
::
++  patch
  |=  [id=@ud rev=@ud name=@t]
  %-  pairs:enjs:format
  :~  fn+s+'file.patch'
      args+(pairs:enjs:format ~[id+(numb:enjs:format id) 'expectedRevision'^(numb:enjs:format rev) name+s+name])
  ==
::
++  test-create-and-list
  %-  eval-mare
  =/  m  (mare ,~)
  ^-  form:m
  ;<  ~  bind:m  init
  ;<  *  bind:m  (call create)
  ;<  =cage  bind:m  (got-peek /x/files)
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
  ;<  *  bind:m  (call (patch 1 0 'renamed.pdf'))
  ;<  *  bind:m  (call (patch 1 0 'stale.pdf'))
  ;<  =cage  bind:m  (got-peek /x/files)
  =/  jon=json  !<(json q.cage)
  ?>  ?=(%a -.jon)
  ?>  ?=([^ ~] p.jon)
  ?>  ?=(%o -.i.p.jon)
  =/  rev=json  (~(got by p.i.p.jon) 'revision')
  ?>  ?=(%n -.rev)
  ?>  =('1' p.rev)
  (pure:m ~)
--
