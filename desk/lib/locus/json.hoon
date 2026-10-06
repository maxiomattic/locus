/-  loc=locus
|%
++  enjs
  =,  enjs:format
  |%
  ++  role
    |=  r=role:loc
    ^-  json
    s+r
  ::
  ++  member
    |=  m=member:loc
    ^-  json
    %-  pairs
    :~  ship+s+(scot %p who.m)
        name+s+name.m
        role+(role role.m)
    ==
  ::
  ++  members
    |=  m=(map @p member:loc)
    ^-  json
    a+(turn ~(val by m) member)
  ::
  ++  job
    |=  j=job:loc
    ^-  json
    =/  by=json  ?~(approved-by.j ~ s+(scot %p u.approved-by.j))
    %-  pairs
    :~  id+(numb id.j)
        at+(time at.j)
        actor+s+(scot %p actor.j)
        fn+s+fn.j
        args+args.j
        status+s+status.j
        'approvedBy'^by
        result+s+result.j
    ==
  ::
  ++  jobs
    |=  m=(map @ud job:loc)
    ^-  json
    a+(turn ~(val by m) job)
  ::
  ++  pending
    |=  m=(map @ud job:loc)
    ^-  json
    a+(turn (skim ~(val by m) |=(j=job:loc =(%pending status.j))) job)
  ::
  ++  audit
    |=  a=audit:loc
    ^-  json
    %-  pairs
    :~  id+(numb id.a)
        at+(time at.a)
        actor+s+(scot %p actor.a)
        fn+s+fn.a
        args+args.a
        result+s+result.a
    ==
  ::
  ++  audits
    |=  l=(list audit:loc)
    ^-  json
    a+(turn l audit)
  ::
  ++  ok
    |=  extra=(list [@t json])
    ^-  json
    (pairs [[%ok b+&] extra])
  ::
  ++  err
    |=  [typ=@t msg=@t]
    ^-  json
    %-  pairs
    :~  ok+b+|
        error+s+typ
        message+s+msg
    ==
  --
++  dejs
  =,  dejs:format
  |%
  ++  ship-cord
    |=  jon=json
    ^-  @p
    (slav %p (so jon))
  ::
  ++  role
    |=  jon=json
    ^-  role:loc
    =/  t=@t  (so jon)
    ?+  t  ~|(bad-role+t !!)
      %owner   %owner
      %member  %member
      %agent   %agent
    ==
  --
--
