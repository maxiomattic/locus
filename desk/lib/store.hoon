::  Pure logic for %store. No effects, so every rule here can be tested
::  without a ship, and home and client replay to the same result.
::
/-  *store
|%
++  on  ((^on @ud entry) lth)
++  fuel  200.000
++  fs  |=(l=(list [@t json]) ^-(fields (malt l)))
+$  done  (each [objs=(map oid object) undo=(list op)] @t)
::
::  ---- applying operations ----
::
::  Apply a batch atomically: every op, or an error naming the first failure.
::
++  apply
  |=  [objs=(map oid object) seq=@ud at=@da actor=@t ops=(list op)]
  ^-  done
  =/  undo=(list op)  ~
  |-
  ?~  ops  [%& objs undo]
  =/  r  (apply-one objs seq at actor i.ops)
  ?:  ?=(%| -.r)  r
  $(ops t.ops, objs objs.p.r, undo (weld undo.p.r undo))
::
++  apply-one
  |=  [objs=(map oid object) seq=@ud at=@da actor=@t =op]
  ^-  done
  ?:  ?=(%make -.op)
    ?:  =('' id.op)  [%| 'empty id']
    ?:  (~(has by objs) id.op)  [%| (cat 3 'exists: ' id.op)]
    ?^  bad=(type-error objs type.op fields.op)  [%| u.bad]
    =/  clk=(map @t @ud)  (~(run by fields.op) |=(* 1))
    =/  o=object  [id.op type.op 1 (strip fields.op) clk | ~ actor at]
    [%& (~(put by objs) id.op o) [%purge id.op]~]
  =/  got  (~(get by objs) (id-of op))
  ?~  got  [%| (cat 3 'no such object: ' (id-of op))]
  =/  o=object  u.got
  ?-    -.op
      %set
    ?^  bad=(type-error objs type.o fields.op)  [%| u.bad]
    =/  old=fields  fields.o
    =/  new  (write-fields o seq actor base.op fields.op)
    =/  prior=fields
      %-  ~(urn by fields.op)
      |=  [k=@t *]
      (fall (~(get by old) k) ~)
    :+  %&  (~(put by objs) id.op new(rev +(rev.new), by actor, at at))
    [%set id.op 0 prior]~
  ::
      %splice
    =/  cur=(list json)  (as-list (~(gut by fields.o) field.op ~))
    =/  was=json  [%a cur]
    =/  next=(list json)  (skip cur |=(j=json ?=(^ (find [j]~ del.op))))
    =.  next
      |-
      ?~  add.op  next
      $(add.op t.add.op, next (insert next after.i.add.op val.i.add.op))
    =.  o
      %_  o
        fields  (~(put by fields.o) field.op [%a next])
        clock   (~(put by clock.o) field.op +(rev.o))
        rev     +(rev.o)
        by      actor
        at      at
      ==
    [%& (~(put by objs) id.op o) [%set id.op 0 (fs ~[[field.op was]])]~]
  ::
      %trash
    :+  %&  (~(put by objs) id.op o(trashed &, rev +(rev.o), by actor, at at))
    [%restore id.op]~
  ::
      %restore
    :+  %&  (~(put by objs) id.op o(trashed |, rev +(rev.o), by actor, at at))
    [%trash id.op]~
  ::
      %resolve
    =/  mine  (skim clashes.o |=(c=clash =(field.c field.op)))
    ?~  mine  [%| 'no such clash']
    =/  rest  (skip clashes.o |=(c=clash =(field.c field.op)))
    =/  pick=fields
      ?.  keep.op  ~
      (fs ~[[field.op theirs.i.mine]])
    =?  o  keep.op
      o(fields (~(uni by fields.o) pick), clock (~(put by clock.o) field.op +(rev.o)))
    [%& (~(put by objs) id.op o(clashes rest, rev +(rev.o), by actor, at at)) ~]
  ::
      %purge
    [%& (~(del by objs) id.op) ~]
  ==
::
++  id-of
  |=  =op
  ^-  oid
  ?-  -.op
    %make     id.op
    %set      id.op
    %splice   id.op
    %trash    id.op
    %restore  id.op
    %resolve  id.op
    %purge    id.op
  ==
::
::  Write fields, keeping any value that loses to a newer write as a clash.
::  The first write in the home ship's order wins the field.
::
++  write-fields
  |=  [o=object seq=@ud actor=@t base=@ud new=fields]
  ^-  object
  =/  ls=(list [k=@t v=json])  ~(tap by new)
  =/  now=@ud  +(rev.o)
  |-
  ?~  ls  o
  =/  k  k.i.ls
  =/  v  v.i.ls
  =/  stamp=@ud  (~(gut by clock.o) k 0)
  =/  have=json  (~(gut by fields.o) k ~)
  ?:  =(have v)  $(ls t.ls)
  ?:  &(!=(0 base) (gth stamp base))
    $(ls t.ls, clashes.o [[k v actor seq] clashes.o])
  =.  clock.o  (~(put by clock.o) k now)
  =.  fields.o
    ?~  v  (~(del by fields.o) k)
    (~(put by fields.o) k v)
  $(ls t.ls)
::
++  strip
  |=  f=fields
  ^-  fields
  (malt (skip ~(tap by f) |=([* v=json] =(~ v))))
::
++  as-list
  |=  j=json
  ^-  (list json)
  ?:  ?=([%a *] j)  p.j
  ~
::
++  insert
  |=  [l=(list json) after=json val=json]
  ^-  (list json)
  ?~  after  [val l]
  =/  at  (find [after]~ l)
  ?~  at  (snoc l val)
  (into l +(u.at) val)
::
::  ---- types ----
::
::  A type is an object with id type:<name> whose `fields` field maps a
::  field name to a kind. An undeclared type is open.
::
++  type-error
  |=  [objs=(map oid object) type=@t f=fields]
  ^-  (unit @t)
  =/  t  (~(get by objs) (cat 3 'type:' type))
  ?~  t  ~
  =/  decl=json  (~(gut by fields.u.t) 'fields' ~)
  ?.  ?=([%o *] decl)  ~
  =/  ls=(list [k=@t v=json])  ~(tap by f)
  |-
  ?~  ls  ~
  ?~  v.i.ls  $(ls t.ls)
  =/  kind  (~(get by p.decl) k.i.ls)
  ?~  kind  $(ls t.ls)
  ?.  ?=([%s *] u.kind)  $(ls t.ls)
  ?:  (kind-fits p.u.kind v.i.ls)  $(ls t.ls)
  `:((cury cat 3) 'bad value for ' k.i.ls ' in ' type)
::
++  kind-fits
  |=  [kind=@t v=json]
  ^-  ?
  ?+  kind  &
    %text  ?=([%s *] v)
    %ref   ?=([%s *] v)
    %blob  ?=([%s *] v)
    %num   ?=([%n *] v)
    %bool  ?=([%b *] v)
    %list  ?=([%a *] v)
    %refs  ?=([%a *] v)
  ==
::
::  ---- references ----
::
::  Everything that points at an object, found through ref and refs fields of
::  declared types and through the conventional `items` and `ref` fields.
::
++  backlinks
  |=  [objs=(map oid object) id=oid]
  ^-  (list oid)
  %+  murn  ~(tap by objs)
  |=  [k=oid o=object]
  ?.  ?|  =([%s id] (~(gut by fields.o) 'ref' ~))
          ?=(^ (find [[%s id]]~ (as-list (~(gut by fields.o) 'items' ~))))
      ==
    ~
  `k
::
::  ---- permissions ----
::
::  A person may do anything except edit grants when they are not the owner.
::  An agent writes only through a proposal, unless a grant object lets that
::  agent apply these operations to these types by itself.
::
++  allowed
  |=  [objs=(map oid object) owner=? actor=@t ops=(list op)]
  ^-  (each ~ @t)
  ?:  =(%human actor)
    ?:  owner  [%& ~]
    ?:  (lien ops |=(=op (sensitive objs op)))
      [%| 'only the owner can change grants and types']
    [%& ~]
  ?:  (lien ops |=(=op (sensitive objs op)))
    [%| 'an agent cannot change grants or types']
  ?:  (levy ops |=(=op |(?=([%make * %proposal *] op) (granted objs actor op))))  [%& ~]
  [%| 'needs approval']
::
++  type-of
  |=  [objs=(map oid object) =op]
  ^-  @t
  ?:  ?=(%make -.op)  type.op
  type:(~(gut by objs) (id-of op) *object)
::
++  sensitive
  |=  [objs=(map oid object) =op]
  ^-  ?
  ?|  ?=(%purge -.op)
      ?=(^ (find [(type-of objs op)]~ `(list @t)`~['grant' 'type' 'member']))
  ==
::
++  granted
  |=  [objs=(map oid object) actor=@t =op]
  ^-  ?
  =/  type  (type-of objs op)
  %+  lien  ~(val by objs)
  |=  o=object
  ?&  =('grant' type.o)
      !trashed.o
      =([%s actor] (~(gut by fields.o) 'actor' ~))
      (listed (~(gut by fields.o) 'ops' ~) -.op)
      (listed (~(gut by fields.o) 'types' ~) type)
  ==
::
++  listed
  |=  [j=json t=@t]
  ^-  ?
  ?|  ?=(^ (find [[%s t]]~ (as-list j)))
      ?=(^ (find [[%s '*']]~ (as-list j)))
  ==
::
::  ---- scrubbing ----
::
::  An explicit purge removes the object and every trace of it in the log.
::
++  scrub
  |=  [l=log id=oid]
  ^-  log
  %+  gas:on  *log
  %+  turn  (tap:on l)
  |=  [k=@ud e=entry]
  :-  k
  e(ops (skip ops.e (cury mentions id)), undo (skip undo.e (cury mentions id)))
::
++  mentions
  |=  [id=oid =op]
  ^-  ?
  =(id (id-of op))
::
::  ---- views ----
::
::  A view is a live query: {type, where:[{field, is|has|gt|lt}], sort,
::  desc, limit}. It is derived and never stored.
::
++  query
  |=  [objs=(map oid object) q=json]
  ^-  (list object)
  ?.  ?=([%o *] q)  ~
  =/  m=(map @t json)  p.q
  =/  type=json  (~(gut by m) 'type' ~)
  =/  wh=(list json)  (as-list (~(gut by m) 'where' ~))
  =/  all=(list object)
    %+  skim  ~(val by objs)
    |=  o=object
    ?&  !trashed.o
        |(?=(~ type) =([%s type.o] type))
        (levy wh (cury matches o))
    ==
  =/  key=json  (~(gut by m) 'sort' ~)
  =/  sorted=(list object)
    %+  sort  all
    |=  [a=object b=object]
    ?:  ?=([%s *] key)
      (lte-json (~(gut by fields.a) p.key ~) (~(gut by fields.b) p.key ~))
    (aor id.a id.b)
  =?  sorted  =([%b &] (~(gut by m) 'desc' ~))  (flop sorted)
  =/  lim=json  (~(gut by m) 'limit' ~)
  ?.  ?=([%n *] lim)  sorted
  (scag (rash p.lim dem) sorted)
::
++  lte-json
  |=  [a=json b=json]
  ^-  ?
  ?:  &(?=([%n *] a) ?=([%n *] b))
    (lte (num a) (num b))
  ?:  &(?=([%s *] a) ?=([%s *] b))
    (aor p.a p.b)
  (aor a b)
::
++  num
  |=  j=json
  ^-  @ud
  ?.  ?=([%n *] j)  0
  (fall (rush (head (split-dot p.j)) dem) 0)
::
++  split-dot
  |=  t=@t
  ^-  (list @t)
  =/  c  (trip t)
  =/  d  (find "." c)
  ?~  d  ~[t]
  ~[(crip (scag u.d c)) (crip (slag +(u.d) c))]
::
++  matches
  |=  [o=object w=json]
  ^-  ?
  ?.  ?=([%o *] w)  |
  =/  m  p.w
  =/  f  (~(get by m) 'field')
  ?.  ?=([~ %s *] f)  |
  =/  have=json
    ?:  =('id' p.u.f)  [%s id.o]
    (~(gut by fields.o) p.u.f ~)
  ?^  is=(~(get by m) 'is')  =(have u.is)
  ?^  has=(~(get by m) 'has')
    ?:  ?=([%a *] have)  ?=(^ (find [u.has]~ p.have))
    ?.  &(?=([%s *] have) ?=([%s *] u.has))  |
    ?=(^ (find (cass (trip p.u.has)) (cass (trip p.have))))
  ?^  gt=(~(get by m) 'gt')  (gth (num have) (num u.gt))
  ?^  lt=(~(get by m) 'lt')  (lth (num have) (num u.lt))
  |
::
::  ---- proposals ----
::
::  An agent's proposal is an object holding operations nobody has applied.
::
++  proposal-ops
  |=  o=object
  ^-  (unit (list op))
  (ops-d:dejs (~(gut by fields.o) 'ops' ~))
::
++  field-text
  |=  [o=object k=@t]
  ^-  @t
  =/  v=json  (~(gut by fields.o) k ~)
  ?:(?=([%s *] v) p.v '')
::
::  ---- reactions ----
::
::  A view with a `react` field names a tool to run when its result changes.
::
++  reactions
  |=  objs=(map oid object)
  ^-  (list object)
  %+  skim  ~(val by objs)
  |=  o=object
  &(=('view' type.o) !trashed.o (~(has by fields.o) 'react'))
::
++  view-hash
  |=  [objs=(map oid object) v=object]
  ^-  @uv
  %-  sham
  %+  turn  (query objs (~(gut by fields.v) 'query' ~))
  |=(o=object id.o)
::
::  ---- tools ----
::
::  A tool is an object holding a program in a small total language. It can
::  read through queries and can only produce operations: it has no other
::  effects, it is deterministic, and fuel bounds how long it can run. The
::  program is a list of steps run against an environment of named values:
::    {let, be}  {for, as, do}  {when, do}
::    {make: {type, id, fields}}  {set: {id, fields}}
::    {add: {id, field, value}}  {del: {id, field, value}}  {trash: expr}
::
::  Run a tool's program, returning its operations or an error. The ops are
::  produced in order and nothing is applied here.
::
+$  run  [env=(map @t json) ops=(list op) fuel=@ud n=@ud]
::
++  run-tool
  |=  [objs=(map oid object) prog=json args=fields salt=@t]
  ^-  (each (list op) @t)
  =/  r  (steps objs (as-list prog) [args ~ fuel 0] salt)
  ?:  ?=(%| -.r)  r
  [%& (flop ops.p.r)]
::
++  steps
  |=  [objs=(map oid object) ls=(list json) r=run salt=@t]
  ^-  (each run @t)
  ?~  ls  [%& r]
  ?:  =(0 fuel.r)  [%| 'out of fuel']
  =/  x  (step objs i.ls r(fuel (dec fuel.r)) salt)
  ?:  ?=(%| -.x)  x
  $(ls t.ls, r p.x)
::
++  step
  |=  [objs=(map oid object) s=json r=run salt=@t]
  ^-  (each run @t)
  ?.  ?=([%o *] s)  [%| 'a step must be an object']
  =/  m  p.s
  =/  ev  |=(k=@t (expr objs (~(gut by m) k ~) env.r))
  ?^  l=(~(get by m) 'let')
    ?.  ?=([%s *] u.l)  [%| 'let needs a name']
    [%& r(env (~(put by env.r) p.u.l (ev 'be')))]
  ?^  f=(~(get by m) 'for')
    =/  xs  (as-list (expr objs u.f env.r))
    =/  nam=json  (~(gut by m) 'as' s+'it')
    ?.  ?=([%s *] nam)  [%| 'as needs a name']
    =/  body  (as-list (~(gut by m) 'do' ~))
    |-
    ?~  xs  [%& r]
    =/  x  (steps objs body r(env (~(put by env.r) p.nam i.xs)) salt)
    ?:  ?=(%| -.x)  x
    $(xs t.xs, r p.x)
  ?^  w=(~(get by m) 'when')
    ?.  (truthy (expr objs u.w env.r))  [%& r]
    (steps objs (as-list (~(gut by m) 'do' ~)) r salt)
  ?^  a=(~(get by m) 'make')
    =/  sub  (sub-ev objs u.a env.r)
    =/  type  (sub 'type')
    ?.  ?=([%s *] type)  [%| 'make needs a type']
    =/  id=json  (sub 'id')
    =/  name=@t
      ?:  ?=([%s *] id)  p.id
      (rap 3 ~[salt '-' (crip (a-co:co n.r))])
    =/  fs=json  (sub 'fields')
    ?.  ?=([%o *] fs)  [%| 'make needs fields']
    [%& r(ops [[%make name p.type p.fs] ops.r], n +(n.r))]
  ?^  a=(~(get by m) 'set')
    =/  sub  (sub-ev objs u.a env.r)
    =/  id  (sub 'id')
    ?.  ?=([%s *] id)  [%| 'set needs an id']
    =/  fs  (sub 'fields')
    ?.  ?=([%o *] fs)  [%| 'set needs fields']
    [%& r(ops [[%set p.id 0 p.fs] ops.r])]
  ?^  a=(~(get by m) 'add')
    =/  sub  (sub-ev objs u.a env.r)
    =/  id  (sub 'id')
    =/  fd  (sub 'field')
    ?.  &(?=([%s *] id) ?=([%s *] fd))  [%| 'add needs id and field']
    [%& r(ops [[%splice p.id p.fd ~ ~[[~ (sub 'value')]]] ops.r])]
  ?^  a=(~(get by m) 'del')
    =/  sub  (sub-ev objs u.a env.r)
    =/  id  (sub 'id')
    =/  fd  (sub 'field')
    ?.  &(?=([%s *] id) ?=([%s *] fd))  [%| 'del needs id and field']
    [%& r(ops [[%splice p.id p.fd ~[(sub 'value')] ~] ops.r])]
  ?^  a=(~(get by m) 'trash')
    =/  id  (expr objs u.a env.r)
    ?.  ?=([%s *] id)  [%| 'trash needs an id']
    [%& r(ops [[%trash p.id] ops.r])]
  [%| 'unknown step']
::
++  sub-ev
  |=  [objs=(map oid object) j=json env=(map @t json)]
  |=  k=@t
  ^-  json
  ?.  ?=([%o *] j)  ~
  (expr objs (~(gut by p.j) k ~) env)
::
++  truthy
  |=  j=json
  ^-  ?
  ?+  j  &
    ~        |
    [%b %.n]  |
    [%a ~]   |
    [%s %$]  |
  ==
::
::  Expressions. A plain string or number is itself; {var}, {get}, {query}
::  and the rest compute. Objects are built with {obj: {...}}.
::
++  expr
  |=  [objs=(map oid object) e=json env=(map @t json)]
  ^-  json
  ?.  ?=([%o *] e)  e
  =/  m  p.e
  =/  ev  |=(j=json (expr objs j env))
  ?^  v=(~(get by m) 'var')
    ?.  ?=([%s *] u.v)  ~
    (~(gut by env) p.u.v ~)
  ?^  v=(~(get by m) 'lit')  u.v
  ?^  v=(~(get by m) 'obj')
    ?.  ?=([%o *] u.v)  ~
    [%o (~(run by p.u.v) ev)]
  ?^  v=(~(get by m) 'arr')
    [%a (turn (as-list u.v) ev)]
  ?^  v=(~(get by m) 'get')
    =/  l  (as-list u.v)
    ?.  ?=([* * ~] l)  ~
    =/  id  (ev i.l)
    =/  f   (ev i.t.l)
    ?.  &(?=([%s *] id) ?=([%s *] f))  ~
    =/  o  (~(get by objs) p.id)
    ?~  o  ~
    ?:  =('id' p.f)  id
    ?:  =('type' p.f)  [%s type.u.o]
    (~(gut by fields.u.o) p.f ~)
  ?^  v=(~(get by m) 'query')
    [%a (turn (query objs (ev u.v)) |=(o=object [%s id.o]))]
  ?^  v=(~(get by m) 'concat')
    :-  %s
    %+  rap  3
    %+  turn  (as-list u.v)
    |=  j=json
    =/  x  (ev j)
    ?+  x  ''
      [%s *]  p.x
      [%n *]  p.x
    ==
  ?^  v=(~(get by m) 'lower')
    =/  x  (ev u.v)
    ?.  ?=([%s *] x)  ~
    [%s (crip (cass (trip p.x)))]
  ?^  v=(~(get by m) 'eq')
    =/  l  (as-list u.v)
    ?.  ?=([* * ~] l)  ~
    [%b =((ev i.l) (ev i.t.l))]
  ?^  v=(~(get by m) 'not')  [%b !(truthy (ev u.v))]
  ?^  v=(~(get by m) 'len')  [%n (crip (a-co:co (lent (as-list (ev u.v)))))]
  ?^  v=(~(get by m) 'if')
    =/  l  (as-list u.v)
    ?.  ?=([* * * ~] l)  ~
    ?:((truthy (ev i.l)) (ev i.t.l) (ev i.t.t.l))
  ?^  v=(~(get by m) 'plus')
    =/  l  (as-list u.v)
    ?.  ?=([* * ~] l)  ~
    [%n (crip (a-co:co (add (num (ev i.l)) (num (ev i.t.l)))))]
  ?^  v=(~(get by m) 'mod')
    =/  l  (as-list u.v)
    ?.  ?=([* * ~] l)  ~
    =/  d  (num (ev i.t.l))
    ?:  =(0 d)  ~
    [%n (crip (a-co:co (mod (num (ev i.l)) d)))]
  e
::
::  What a tool's program can do, read from the program without running it:
::  the kinds of operation and the types it can make. A tool declares these
::  as `can`, and is refused when the program goes beyond them.
::
++  effects
  |=  prog=json
  ^-  (set @t)
  =/  ls=(list json)  (as-list prog)
  =/  out=(set @t)  ~
  |-
  ?~  ls  out
  ?.  ?=([%o *] i.ls)  $(ls t.ls)
  =/  m  p.i.ls
  =.  out  (~(uni in out) (effects (~(gut by m) 'do' ~)))
  =.  out
    %-  ~(gas in out)
    %+  murn  ~['set' 'add' 'del' 'trash']
    |=  k=@t
    ?.((~(has by m) k) ~ `?:(?=(?(%add %del) k) 'splice' k))
  =?  out  (~(has by m) 'make')
    =/  t=json  (~(gut by (mk (~(got by m) 'make'))) 'type' ~)
    (~(put in out) ?:(?=([%s *] t) (cat 3 'make:' p.t) 'make:*'))
  $(ls t.ls)
::
++  mk
  |=  j=json
  ^-  (map @t json)
  ?.(?=([%o *] j) ~ p.j)
::
++  over-reach
  |=  [can=json prog=json]
  ^-  (list @t)
  %+  skip  ~(tap in (effects prog))
  |=(e=@t ?=(^ (find [[%s e]]~ (as-list can))))
::
::  Check a tool against its examples: each runs the program on `args` over
::  `given` objects' current state and must yield `expect` op kinds in order.
::
++  check-tool
  |=  [objs=(map oid object) t=object]
  ^-  (unit @t)
  =/  prog=json  (~(gut by fields.t) 'program' ~)
  =/  over  (over-reach (~(gut by fields.t) 'can' ~) prog)
  ?^  over  `(cat 3 'program exceeds what it declares: ' i.over)
  =/  ex=(list json)  (as-list (~(gut by fields.t) 'examples' ~))
  |-
  ?~  ex  ~
  =/  m  (mk i.ex)
  =/  args=fields  (mk (~(gut by m) 'args' ~))
  =/  r  (run-tool objs prog args 'check')
  ?:  ?=(%| -.r)  `p.r
  =/  want  (as-list (~(gut by m) 'expect' ~))
  =/  got=(list json)  (turn p.r |=(=op [%s -.op]))
  ?.  =(want got)  `'an example gave different operations'
  $(ex t.ex)
::
::  ---- json ----
::
++  enjs
  =,  enjs:format
  |%
  ++  object-j
    |=  o=object
    ^-  json
    %-  pairs
    :~  id+s+id.o
        type+s+type.o
        rev+(numb rev.o)
        fields+[%o fields.o]
        trashed+b+trashed.o
        by+s+by.o
        at+(sect at.o)
        :-  'clashes'
        :-  %a
        %+  turn  clashes.o
        |=  c=clash
        (pairs ~[field+s+field.c theirs+theirs.c actor+s+actor.c seq+(numb seq.c)])
    ==
  ++  op-j
    |=  o=op
    ^-  json
    ?-  -.o
      %make     (pairs ~[do+s+'make' id+s+id.o type+s+type.o fields+[%o fields.o]])
      %set      (pairs ~[do+s+'set' id+s+id.o base+(numb base.o) fields+[%o fields.o]])
      %trash    (pairs ~[do+s+'trash' id+s+id.o])
      %restore  (pairs ~[do+s+'restore' id+s+id.o])
      %purge    (pairs ~[do+s+'purge' id+s+id.o])
      %resolve  (pairs ~[do+s+'resolve' id+s+id.o field+s+field.o keep+b+keep.o])
        %splice
      %-  pairs
      :~  do+s+'splice'
          id+s+id.o
          field+s+field.o
          del+[%a del.o]
          add+[%a (turn add.o |=([a=json v=json] (pairs ~[after+a val+v])))]
      ==
    ==
  ++  entry-j
    |=  e=entry
    ^-  json
    %-  pairs
    :~  seq+(numb seq.e)
        at+(sect at.e)
        who+s+(scot %p who.e)
        actor+s+actor.e
        intent+s+intent.e
        cid+s+cid.e
        ops+[%a (turn ops.e op-j)]
        undo+[%a (turn undo.e op-j)]
    ==
  --
::
++  dejs
  |%
  ++  op-d
    |=  j=json
    ^-  (unit op)
    ?.  ?=([%o *] j)  ~
    =/  m  p.j
    =/  t  |=(k=@t =/(v (~(gut by m) k ~) ?:(?=([%s *] v) p.v '')))
    =/  id=@t  (t 'id')
    =/  f=(map @t json)  (mk (~(gut by m) 'fields' ~))
    ?+  (t 'do')  ~
      %make     `[%make id (t 'type') f]
      %set      `[%set id (num (~(gut by m) 'base' ~)) f]
      %trash    `[%trash id]
      %restore  `[%restore id]
      %purge    `[%purge id]
      %resolve  `[%resolve id (t 'field') =([%b &] (~(gut by m) 'keep' ~))]
        %splice
      :+  ~  %splice
      :^  id  (t 'field')  (as-list (~(gut by m) 'del' ~))
      %+  turn  (as-list (~(gut by m) 'add' ~))
      |=  a=json
      [(~(gut by (mk a)) 'after' ~) (~(gut by (mk a)) 'val' ~)]
    ==
  ++  ops-d
    |=  j=json
    ^-  (unit (list op))
    =/  ls  (as-list j)
    =/  out=(list op)  ~
    |-
    ?~  ls  `(flop out)
    =/  o  (op-d i.ls)
    ?~  o  ~
    $(ls t.ls, out [u.o out])
  --
--
