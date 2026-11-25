# The core of SketchUp's Ruby API, for running SketchUp scripts in SketchCraft.
#
# Scripts run against a copy of what they build (lengths in inches, as in
# SketchUp) and each change is recorded as a SketchCraft command; the app then
# replays the commands, so one undo step per command, and nothing changes if
# the script fails. Compiled with Opal; runs in the plugin sandbox (no files,
# no network). What isn't here raises a clear "not supported yet" error.

module SC
  M_PER_IN = 0.0254
  @n = 0
  @ctx = nil # the group being added to (nil: the model)

  def self.name_for
    @n += 1
    "rb#{`globalThis.__runId`}_#{@n}"
  end

  # a Ruby value as a plain JavaScript one (for the journal)
  def self.js(v)
    if v.is_a?(Hash)
      o = `{}`
      v.each { |k, x| `#{o}[#{k.to_s}] = #{SC.js(x)}` }
      o
    elsif v.is_a?(Array)
      v.map { |x| SC.js(x) }
    elsif v.nil?
      `null`
    elsif v.is_a?(Symbol) || v.is_a?(String)
      v.to_s
    else
      v
    end
  end

  # record one SketchCraft command, inside the right group
  # (a script can't queue more than MAX_COMMANDS: each one is replayed, and a loop could add millions)
  MAX_COMMANDS = 10_000

  def self.emit(cmd, ctx = :same)
    raise "this script made more than #{MAX_COMMANDS} changes in one run, so it was stopped" if `globalThis.__journal.length` > MAX_COMMANDS
    if ctx != :same && ctx != @ctx
      `globalThis.__journal.push({cmd: "close"})` if @ctx
      `globalThis.__journal.push({cmd: "edit", id: #{ctx.jid}})` if ctx
      @ctx = ctx
    end
    `globalThis.__journal.push(#{SC.js(cmd)})`
  end

  # leave any open group at the end
  def self.finish
    `globalThis.__journal.push({cmd: "close"})` if @ctx
    @ctx = nil
  end

  def self.m(p)
    a = SC.pt(p)
    [a[0] * M_PER_IN, a[1] * M_PER_IN, a[2] * M_PER_IN]
  end

  def self.pt(p)
    return [p.x.to_f, p.y.to_f, p.z.to_f] if p.respond_to?(:x) && !p.is_a?(Array)
    a = p.to_a
    [a[0].to_f, a[1].to_f, (a[2] || 0).to_f]
  end

  def self.vec(v)
    a = SC.pt(v)
    Geom::Vector3d.new(a[0], a[1], a[2])
  end

  def self.toast(text)
    `globalThis.__toasts.push(#{text.to_s})`
  end

  def self.unsupported(what)
    raise NotImplementedError, "#{what} isn't supported in SketchCraft yet"
  end

  # a color as 0..1 floats
  def self.rgb(c)
    c = Sketchup::Color.new(c) unless c.is_a?(Sketchup::Color)
    [c.red / 255.0, c.green / 255.0, c.blue / 255.0]
  end

  # what a face, color or name paints with
  def self.paint(ids, m)
    if m.nil?
      SC.emit({ cmd: "paint", ids: ids, color: [0.95, 0.94, 0.91] })
    elsif m.is_a?(Sketchup::Material)
      SC.emit({ cmd: "paint", ids: ids, material: m.name })
    elsif m.is_a?(String) && Sketchup.active_model.materials[m]
      SC.emit({ cmd: "paint", ids: ids, material: m })
    else
      SC.emit({ cmd: "paint", ids: ids, color: SC.rgb(m) })
    end
  end
end

class Numeric
  def inch; to_f; end
  def feet; self * 12.0; end
  def yard; self * 36.0; end
  def m; self / 0.0254; end
  def cm; self / 2.54; end
  def mm; self / 25.4; end
  def km; self / 0.0000254; end
  def mile; self * 63_360.0; end
  def to_inch; to_f; end
  def to_feet; self / 12.0; end
  def to_m; self * 0.0254; end
  def to_cm; self * 2.54; end
  def to_mm; self * 25.4; end
  def to_l; to_f; end
  def degrees; self * Math::PI / 180.0; end
  def radians; self * 180.0 / Math::PI; end
end

class String
  # a length typed with units ("2.5m", "8'", "90cm", "3\"") in inches
  def to_l
    s = strip.downcase
    return $1.to_f * 12.0 + $2.to_f if s =~ /\A(-?[\d.]+)\s*'\s*(-?[\d.]+)\s*"?\z/
    return $1.to_f * 12.0 if s =~ /\A(-?[\d.]+)\s*('|ft|feet)\z/
    return $1.to_f if s =~ /\A(-?[\d.]+)\s*("|in|inch|inches)?\z/
    return $1.to_f / 25.4 if s =~ /\A(-?[\d.]+)\s*mm\z/
    return $1.to_f / 2.54 if s =~ /\A(-?[\d.]+)\s*cm\z/
    return $1.to_f / 0.0254 if s =~ /\A(-?[\d.]+)\s*m\z/
    raise ArgumentError, "can't read #{inspect} as a length"
  end
end

class Array
  def x; self[0]; end
  def y; self[1]; end
  def z; self[2] || 0; end
  def distance(p); Geom::Point3d.new(self).distance(p); end
  def vector_to(p); Geom::Point3d.new(p) - Geom::Point3d.new(self); end
  def offset(v, len = nil); Geom::Point3d.new(self).offset(v, len).to_a; end
  def transform(t); t * Geom::Point3d.new(self); end
end

module Geom
  class Point3d
    attr_accessor :x, :y, :z
    def initialize(*a)
      a = a[0].respond_to?(:to_a) ? SC.pt(a[0]) : [0, 0, 0] if a.length <= 1
      @x = a[0].to_f
      @y = a[1].to_f
      @z = (a[2] || 0).to_f
    end
    def to_a; [@x, @y, @z]; end
    def [](i); to_a[i]; end
    def +(v); v = SC.pt(v); Point3d.new(@x + v[0], @y + v[1], @z + v[2]); end
    def -(o)
      a = SC.pt(o)
      return Vector3d.new(@x - a[0], @y - a[1], @z - a[2]) if o.is_a?(Point3d) || o.is_a?(Array)
      Point3d.new(@x - a[0], @y - a[1], @z - a[2])
    end
    def distance(p); a = SC.pt(p); Math.sqrt((@x - a[0])**2 + (@y - a[1])**2 + (@z - a[2])**2); end
    def offset(v, len = nil)
      v = SC.vec(v)
      v = v.normalize.scale(len) if len
      self + v
    end
    def vector_to(p); Point3d.new(p) - self; end
    def transform(t); t * self; end
    def ==(o); o.respond_to?(:to_a) && distance(o) < 0.001; end
    def to_s; "Point3d(#{@x}, #{@y}, #{@z})"; end
    def inspect; to_s; end
  end

  class Vector3d
    attr_accessor :x, :y, :z
    def initialize(*a)
      a = a[0].respond_to?(:to_a) ? SC.pt(a[0]) : [0, 0, 0] if a.length <= 1
      @x = a[0].to_f
      @y = a[1].to_f
      @z = (a[2] || 0).to_f
    end
    def to_a; [@x, @y, @z]; end
    def [](i); to_a[i]; end
    def length; Math.sqrt(@x * @x + @y * @y + @z * @z); end
    def length=(l); n = normalize; @x, @y, @z = n.x * l, n.y * l, n.z * l; end
    def valid?; length > 1e-9; end
    def normalize; l = length; l < 1e-12 ? Vector3d.new(0, 0, 0) : Vector3d.new(@x / l, @y / l, @z / l); end
    def normalize!; n = normalize; @x, @y, @z = n.x, n.y, n.z; self; end
    def scale(k); Vector3d.new(@x * k, @y * k, @z * k); end
    def reverse; Vector3d.new(-@x, -@y, -@z); end
    def reverse!; @x, @y, @z = -@x, -@y, -@z; self; end
    def +(v); v = SC.pt(v); Vector3d.new(@x + v[0], @y + v[1], @z + v[2]); end
    def -(v); v = SC.pt(v); Vector3d.new(@x - v[0], @y - v[1], @z - v[2]); end
    def dot(v); v = SC.pt(v); @x * v[0] + @y * v[1] + @z * v[2]; end
    alias % dot
    def cross(v); v = SC.pt(v); Vector3d.new(@y * v[2] - @z * v[1], @z * v[0] - @x * v[2], @x * v[1] - @y * v[0]); end
    alias * cross
    def angle_between(v); Math.acos([[normalize.dot(SC.vec(v).normalize), 1.0].min, -1.0].max); end
    def parallel?(v); cross(SC.vec(v)).length < 1e-6 * [length * SC.vec(v).length, 1e-9].max; end
    def samedirection?(v); parallel?(v) && dot(v) > 0; end
    def perpendicular?(v); dot(v).abs < 1e-6; end
    def unitvector?; (length - 1).abs < 1e-6; end
    def transform(t); t.apply_vector(self); end
    def to_s; "Vector3d(#{@x}, #{@y}, #{@z})"; end
    def inspect; to_s; end
  end

  # A transformation: what it does to points, and the moves that make it
  # (so a group can be moved, turned or scaled with SketchCraft commands).
  class Transformation
    attr_reader :ops
    def initialize(arg = nil, ops = nil)
      @ops = ops || []
      if arg.respond_to?(:to_a) && arg.to_a.length == 3 && ops.nil?
        @ops = [[:move, SC.pt(arg)]]
      elsif arg.is_a?(Array) && arg.length == 16
        SC.unsupported("Geom::Transformation from a matrix")
      end
    end
    def self.translation(v); new(nil, [[:move, SC.pt(v)]]); end
    def self.rotation(pt, axis, angle); new(nil, [[:rotate, SC.pt(pt), SC.pt(axis), angle.to_f]]); end
    def self.scaling(*a)
      case a.length
      when 1 then new(nil, [[:scale, [0, 0, 0], [a[0].to_f] * 3]])
      when 2 then new(nil, [[:scale, SC.pt(a[0]), [a[1].to_f] * 3]])
      when 3 then new(nil, [[:scale, [0, 0, 0], a.map(&:to_f)]])
      when 4 then new(nil, [[:scale, SC.pt(a[0]), a[1..3].map(&:to_f)]])
      else raise ArgumentError, "scaling takes 1 to 4 values"
      end
    end
    def identity?; @ops.empty?; end
    def origin; self * Geom::Point3d.new(0, 0, 0); end
    # self * other: other first, then self
    def *(o)
      return Transformation.new(nil, o.ops + @ops) if o.is_a?(Transformation)
      p = SC.pt(o)
      @ops.each { |op| p = Transformation.apply(op, p) }
      o.is_a?(Array) ? p : Geom::Point3d.new(p)
    end
    def apply_vector(v)
      a = self * Geom::Point3d.new(SC.pt(v))
      b = self * Geom::Point3d.new(0, 0, 0)
      Vector3d.new(a.x - b.x, a.y - b.y, a.z - b.z)
    end
    def inverse
      Transformation.new(nil, @ops.reverse.map do |op|
        case op[0]
        when :move then [:move, op[1].map { |c| -c }]
        when :rotate then [:rotate, op[1], op[2], -op[3]]
        else [:scale, op[1], op[2].map { |f| 1.0 / f }]
        end
      end)
    end
    def self.apply(op, p)
      case op[0]
      when :move then [p[0] + op[1][0], p[1] + op[1][1], p[2] + op[1][2]]
      when :scale
        c, f = op[1], op[2]
        [c[0] + (p[0] - c[0]) * f[0], c[1] + (p[1] - c[1]) * f[1], c[2] + (p[2] - c[2]) * f[2]]
      else
        c, k, t = op[1], Vector3d.new(op[2]).normalize, op[3]
        v = [p[0] - c[0], p[1] - c[1], p[2] - c[2]]
        cos, sin = Math.cos(t), Math.sin(t)
        d = k.x * v[0] + k.y * v[1] + k.z * v[2]
        cr = [k.y * v[2] - k.z * v[1], k.z * v[0] - k.x * v[2], k.x * v[1] - k.y * v[0]]
        r = (0..2).map { |i| v[i] * cos + cr[i] * sin + k.to_a[i] * d * (1 - cos) }
        [c[0] + r[0], c[1] + r[1], c[2] + r[2]]
      end
    end
  end

  class BoundingBox
    attr_reader :min, :max
    def initialize; @min = nil; @max = nil; end
    def add(*pts)
      pts.flatten.each do |q|
        a = SC.pt(q)
        @min = @min ? Geom::Point3d.new([@min.x, a[0]].min, [@min.y, a[1]].min, [@min.z, a[2]].min) : Geom::Point3d.new(a)
        @max = @max ? Geom::Point3d.new([@max.x, a[0]].max, [@max.y, a[1]].max, [@max.z, a[2]].max) : Geom::Point3d.new(a)
      end
      self
    end
    def empty?; @min.nil?; end
    def width; empty? ? 0 : @max.x - @min.x; end
    def height; empty? ? 0 : @max.y - @min.y; end
    def depth; empty? ? 0 : @max.z - @min.z; end
    def center; empty? ? Geom::Point3d.new : Geom::Point3d.new((@min.x + @max.x) / 2, (@min.y + @max.y) / 2, (@min.z + @max.z) / 2); end
  end
end

X_AXIS = Geom::Vector3d.new(1, 0, 0)
Y_AXIS = Geom::Vector3d.new(0, 1, 0)
Z_AXIS = Geom::Vector3d.new(0, 0, 1)
ORIGIN = Geom::Point3d.new(0, 0, 0)
IDENTITY = Geom::Transformation.new

module Sketchup
  class Color
    attr_accessor :red, :green, :blue, :alpha
    NAMES = { "red" => [255, 0, 0], "green" => [0, 128, 0], "blue" => [0, 0, 255], "white" => [255, 255, 255], "black" => [0, 0, 0],
              "gray" => [128, 128, 128], "grey" => [128, 128, 128], "yellow" => [255, 255, 0], "orange" => [255, 165, 0],
              "brown" => [165, 42, 42], "tan" => [210, 180, 140], "beige" => [245, 245, 220], "purple" => [128, 0, 128],
              "pink" => [255, 192, 203], "cyan" => [0, 255, 255], "magenta" => [255, 0, 255], "silver" => [192, 192, 192] }
    def initialize(*a)
      a = a[0] if a.length == 1 && a[0].is_a?(Array)
      if a.length == 1 && a[0].is_a?(String)
        s = a[0].downcase
        a = s.start_with?("#") ? [s[1, 2].to_i(16), s[3, 2].to_i(16), s[5, 2].to_i(16)] : (NAMES[s] || [200, 200, 200])
      elsif a.length == 1 && a[0].is_a?(Color)
        a = [a[0].red, a[0].green, a[0].blue]
      elsif a.length == 1 && a[0].is_a?(Numeric)
        n = a[0].to_i
        a = [n & 255, (n >> 8) & 255, (n >> 16) & 255]
      end
      @red, @green, @blue = a[0].to_i, a[1].to_i, a[2].to_i
      @alpha = (a[3] || 255).to_i
    end
    def to_a; [@red, @green, @blue, @alpha]; end
  end

  class Entity
    attr_reader :ref
    def entityID; @ref; end
    def persistent_id; @ref; end
    # how the journal names this entity: a real SketchCraft id, or "@name" for what the script made
    def jid; @real ? @ref.to_i : "@" + @ref.to_s; end
    def valid?; !@erased; end
    def deleted?; !!@erased; end
    def model; Sketchup.active_model; end
    def attr_dict; @attrs ||= {}; end
    def get_attribute(dict, key, default = nil); (attr_dict[dict] || {}).fetch(key, default); end
    def set_attribute(dict, key, value); (attr_dict[dict] ||= {})[key] = value; value; end
    def attribute_dictionary(name, create = false); attr_dict[name] ||= {} if create; attr_dict[name]; end
    def erase!
      SC.emit({ cmd: "delete", ids: [jid] }) unless @erased
      @erased = true
    end
    def method_missing(name, *_args)
      SC.unsupported("#{self.class.name.split('::').last}##{name}")
    end
    def respond_to_missing?(*_); false; end
  end

  class Drawingelement < Entity
    def material; @material; end
    def material=(m)
      @material = m
      SC.paint([jid], m)
    end
    def layer; @layer; end
    def layer=(l)
      @layer = l
      name = l.respond_to?(:name) ? l.name : l.to_s
      SC.emit({ cmd: "tag", ids: [jid], tag: name == "Layer0" || name == "Untagged" ? nil : name })
    end
    def hidden=(h)
      SC.emit({ cmd: h ? "hide" : "unhide", ids: [jid] })
      @hidden = h
    end
    def hidden?; !!@hidden; end
    def visible?; !@hidden; end
  end

  class Vertex < Entity
    def initialize(p); @position = Geom::Point3d.new(p); end
    def position; @position; end
  end

  class Edge < Drawingelement
    attr_reader :points
    def initialize(ref, a, b)
      @ref = ref
      @points = [Geom::Point3d.new(a), Geom::Point3d.new(b)]
    end
    def start; Vertex.new(@points[0]); end
    def end; Vertex.new(@points[1]); end
    def vertices; [start, self.end]; end
    def length; @points[0].distance(@points[1]); end
    def line; [@points[0], @points[1] - @points[0]]; end
    def soft=(v); SC.emit({ cmd: "soften", ids: [jid], soft: v }) if @ref; end
    def smooth=(v); self.soft = v; end
  end

  # a circle's or arc's edges: one SketchCraft curve
  class CurveEdge < Edge
    attr_reader :face_ref
    def initialize(face_ref, a, b); @face_ref = face_ref; @ref = face_ref; @points = [Geom::Point3d.new(a), Geom::Point3d.new(b)]; end
  end

  class Face < Drawingelement
    attr_reader :outer
    def initialize(ref, pts, normal)
      @ref = ref
      @outer = pts.map { |p| Geom::Point3d.new(p) }
      @normal = normal
    end
    def normal; @normal; end
    def vertices; @outer.map { |p| Vertex.new(p) }; end
    def edges; (0...@outer.length).map { |i| Edge.new(nil, @outer[i], @outer[(i + 1) % @outer.length]) }; end
    def outer_loop; self; end
    def loops; [self]; end
    def plane
      p = @outer[0]
      [@normal.x, @normal.y, @normal.z, -(@normal.x * p.x + @normal.y * p.y + @normal.z * p.z)]
    end
    def area
      s = Geom::Vector3d.new(0, 0, 0)
      @outer.each_with_index { |p, i| s += Geom::Vector3d.new(p.to_a).cross(Geom::Vector3d.new(@outer[(i + 1) % @outer.length].to_a)) }
      (s.dot(@normal) / 2.0).abs
    end
    def bounds; Geom::BoundingBox.new.add(@outer); end
    # pull along the face's normal (negative pushes in)
    def pushpull(d, copy = false)
      SC.emit({ cmd: "pushpull", face: jid, distance: d.to_f * SC::M_PER_IN, toward: @normal.to_a })
      off = @normal.scale(d.to_f)
      @outer = @outer.map { |p| p + off }
      nil
    end
    def reverse!
      @normal = @normal.reverse
      SC.emit({ cmd: "reverse", ids: [jid] })
      self
    end
    def back_material=(m); self.material = m; end
  end

  class Entities
    include Enumerable
    attr_reader :owner
    def initialize(owner); @owner = owner; @items = []; end
    def each(&b); @items.reject(&:deleted?).each(&b); end
    def to_a; @items.reject(&:deleted?); end
    def length; to_a.length; end
    alias size length
    alias count length
    def [](i); to_a[i]; end
    def model; Sketchup.active_model; end
    def parent; @owner || Sketchup.active_model; end
    def ctx; @owner.is_a?(Group) ? @owner : nil; end

    def keep(e); @items.push(e); e; end

    def add_face(*args)
      args = args[0] if args.length == 1 && args[0].is_a?(Array) && args[0][0].respond_to?(:to_a) && !args[0][0].is_a?(Numeric)
      if args.all? { |a| a.is_a?(CurveEdge) } && args.length > 0 && args.map(&:face_ref).uniq.length == 1
        # a circle's edges: SketchCraft's circle already made the face
        e = args[0]
        return keep(Face.new(e.face_ref, args.map { |x| x.points[0] }, e.instance_variable_get(:@normal) || Z_AXIS))
      end
      pts = args.flat_map { |a| a.is_a?(Edge) ? [a.points[0]] : [a] }.map { |p| SC.pt(p) }
      raise ArgumentError, "a face needs at least 3 points" if pts.length < 3
      ref = SC.name_for
      SC.emit({ cmd: "polygon", points: pts.map { |p| SC.m(p) }, as: ref }, ctx)
      keep(Face.new(ref, pts, Entities.normal_of(pts)))
    end

    # Newell's normal; a face lying on the ground faces down, as in SketchUp
    def self.normal_of(pts)
      n = [0.0, 0.0, 0.0]
      pts.each_with_index do |p, i|
        q = pts[(i + 1) % pts.length]
        n[0] += (p[1] - q[1]) * (p[2] + q[2])
        n[1] += (p[2] - q[2]) * (p[0] + q[0])
        n[2] += (p[0] - q[0]) * (p[1] + q[1])
      end
      v = Geom::Vector3d.new(n).normalize
      v = Geom::Vector3d.new(0, 0, -1) if pts.all? { |p| p[2].abs < 1e-6 } && v.z.abs > 0.999
      v
    end

    def add_line(a, b)
      ref = SC.name_for
      SC.emit({ cmd: "line", points: [SC.m(a), SC.m(b)], as: ref }, ctx)
      keep(Edge.new(ref, SC.pt(a), SC.pt(b)))
    end

    def add_edges(*pts)
      pts = pts[0] if pts.length == 1 && pts[0].is_a?(Array) && !pts[0][0].is_a?(Numeric)
      ref = SC.name_for
      SC.emit({ cmd: "line", points: pts.map { |p| SC.m(p) }, as: ref }, ctx)
      (0...pts.length - 1).map { |i| keep(Edge.new(i == 0 ? ref : nil, SC.pt(pts[i]), SC.pt(pts[i + 1]))) }
    end
    alias add_curve add_edges

    def add_circle(center, normal, radius, segs = 24)
      ref = SC.name_for
      n = SC.vec(normal).normalize
      SC.emit({ cmd: "circle", center: SC.m(center), normal: n.to_a, radius: radius.to_f * SC::M_PER_IN, segments: segs.to_i, as: ref }, ctx)
      pts = Entities.ring(center, n, radius.to_f, segs.to_i)
      (0...pts.length).map { |i| e = CurveEdge.new(ref, pts[i], pts[(i + 1) % pts.length]); e.instance_variable_set(:@normal, n); keep(e) }
    end

    def add_ngon(center, normal, radius, n = 6); add_circle(center, normal, radius, n); end

    def add_arc(center, xaxis, normal, radius, a0, a1, segs = 12)
      c = SC.pt(center)
      x = SC.vec(xaxis).normalize
      z = SC.vec(normal).normalize
      y = z.cross(x).normalize
      pts = (0..segs).map do |i|
        t = a0 + (a1 - a0) * i / segs.to_f
        [c[0] + radius * (Math.cos(t) * x.x + Math.sin(t) * y.x), c[1] + radius * (Math.cos(t) * x.y + Math.sin(t) * y.y), c[2] + radius * (Math.cos(t) * x.z + Math.sin(t) * y.z)]
      end
      add_edges(pts)
    end

    def self.ring(center, n, r, segs)
      c = SC.pt(center)
      u = (n.parallel?(Z_AXIS) ? X_AXIS : n.cross(Z_AXIS)).normalize
      v = n.cross(u).normalize
      (0...segs).map do |i|
        t = 2 * Math::PI * i / segs
        [c[0] + r * (Math.cos(t) * u.x + Math.sin(t) * v.x), c[1] + r * (Math.cos(t) * u.y + Math.sin(t) * v.y), c[2] + r * (Math.cos(t) * u.z + Math.sin(t) * v.z)]
      end
    end

    def add_group(*ents)
      ents = ents.flatten
      ref = SC.name_for
      if ents.empty?
        SC.emit({ cmd: "begin_group", name: "Group", as: ref }, ctx)
        SC.emit({ cmd: "end_group" })
      else
        SC.emit({ cmd: "group", ids: ents.map { |e| e.jid }, as: ref }, ctx)
      end
      keep(Group.new(ref, ctx))
    end

    def add_instance(defn, t)
      keep(defn.place(t, ctx))
    end

    def add_cpoint(p)
      ref = SC.name_for
      SC.emit({ cmd: "guide", point: SC.m(p), as: ref }, ctx)
      keep(Drawingelement.new.tap { |e| e.instance_variable_set(:@ref, ref) })
    end

    def add_cline(p, v, *_)
      ref = SC.name_for
      SC.emit({ cmd: "guide", point: SC.m(p), direction: SC.vec(v).normalize.to_a, as: ref }, ctx)
      keep(Drawingelement.new.tap { |e| e.instance_variable_set(:@ref, ref) })
    end

    def add_text(text, p, v = nil)
      ref = SC.name_for
      at = v ? SC.pt(Geom::Point3d.new(p) + SC.vec(v)) : SC.pt(p)
      cmd = { cmd: "text", at: SC.m(at), text: text.to_s, as: ref }
      cmd[:leader] = SC.m(p) if v
      SC.emit(cmd, ctx)
      keep(Drawingelement.new.tap { |e| e.instance_variable_set(:@ref, ref) })
    end

    def add_dimension_linear(a, b, offset)
      ref = SC.name_for
      SC.emit({ cmd: "dimension", from: SC.m(a), to: SC.m(b), offset: SC.m(SC.pt(offset)), as: ref }, ctx)
      keep(Drawingelement.new.tap { |e| e.instance_variable_set(:@ref, ref) })
    end

    def erase_entities(*ents)
      list = ents.flatten.reject(&:deleted?)
      return if list.empty?
      SC.emit({ cmd: "delete", ids: list.map { |e| e.jid } })
      list.each { |e| e.instance_variable_set(:@erased, true) }
    end

    def clear!; erase_entities(to_a); end
    def transform_entities(t, *ents)
      ents.flatten.each { |e| e.respond_to?(:transform!) ? e.transform!(t) : SC.unsupported("transforming a #{e.class.name}") }
    end
    def method_missing(name, *_); SC.unsupported("Entities##{name}"); end
    def respond_to_missing?(*_); false; end
  end

  class Group < Drawingelement
    attr_reader :entities, :parent_ctx
    def initialize(ref, parent_ctx, name = "Group")
      @ref = ref
      @parent_ctx = parent_ctx
      @entities = Entities.new(self)
      @name = name
    end
    def name; @name; end
    def name=(n); @name = n.to_s; SC.emit({ cmd: "rename", id: jid, name: @name }); end
    def definition; self; end
    def transform!(t)
      t.ops.each do |op|
        case op[0]
        when :move then SC.emit({ cmd: "move", ids: [jid], by: SC.m(op[1]) }, @parent_ctx)
        when :rotate then SC.emit({ cmd: "rotate", ids: [jid], center: SC.m(op[1]), axis: SC.vec(op[2]).normalize.to_a, angle: op[3] * 180.0 / Math::PI }, @parent_ctx)
        else SC.emit({ cmd: "scale", ids: [jid], center: SC.m(op[1]), factor: op[2] }, @parent_ctx)
        end
      end
      self
    end
    def move!(t); transform!(t); end
    def explode; SC.emit({ cmd: "explode", id: jid }); []; end
    def to_component
      ref = SC.name_for
      SC.emit({ cmd: "make_component", ids: [jid], name: @name, as: ref }, @parent_ctx)
      ComponentInstance.new(ref, @parent_ctx, nil)
    end
    def bounds; Geom::BoundingBox.new; end
    def locked=(v); SC.emit({ cmd: "lock", ids: [jid], locked: v }); end
  end

  class ComponentDefinition < Entity
    attr_reader :name, :entities
    def initialize(name)
      @name = name
      @group = nil
      @entities = DefinitionEntities.new(self)
      @instances = []
    end
    def instances; @instances; end
    def group
      # what the definition holds is built in a group, made a component when first placed
      @group ||= begin
        ref = SC.name_for
        SC.emit({ cmd: "begin_group", name: @name, as: ref }, nil)
        SC.emit({ cmd: "end_group" })
        Group.new(ref, nil, @name)
      end
    end
    def place(t, ctx)
      first = @instances.empty?
      if first
        ref = SC.name_for
        SC.emit({ cmd: "make_component", ids: [group.jid], name: @name, as: ref }, nil)
        @comp = ref
        inst = ComponentInstance.new(ref, nil, self)
        inst.transform!(t) unless t.nil? || (t.respond_to?(:identity?) && t.identity?)
      else
        ref = SC.name_for
        at = t ? (t * Geom::Point3d.new(0, 0, 0)) : Geom::Point3d.new(0, 0, 0)
        SC.emit({ cmd: "place", component: @name, at: SC.m(at), as: ref }, ctx)
        inst = ComponentInstance.new(ref, ctx, self)
      end
      @instances.push(inst)
      inst
    end
  end

  # what a definition holds goes into its building group
  class DefinitionEntities < Entities
    def ctx; @owner.group; end
  end

  class ComponentInstance < Group
    def initialize(ref, parent_ctx, defn)
      super(ref, parent_ctx, defn ? defn.name : "Component")
      @definition = defn
    end
    def definition; @definition; end
  end

  class Definitions
    include Enumerable
    def initialize; @all = {}; end
    def add(name); @all[name.to_s] ||= ComponentDefinition.new(name.to_s); end
    def [](name); @all[name.to_s]; end
    def each(&b); @all.values.each(&b); end
    def length; @all.length; end
    def load(*_); SC.unsupported("DefinitionList#load"); end
  end

  class Material
    attr_reader :name, :color
    def initialize(name); @name = name; @color = Color.new(200, 200, 200); end
    def display_name; @name; end
    def color=(c)
      @color = c.is_a?(Color) ? c : Color.new(c)
      SC.emit({ cmd: "material", name: @name, color: SC.rgb(@color) })
    end
    def alpha=(a); SC.emit({ cmd: "opacity", material: @name, opacity: a.to_f }); end
  end

  class Materials
    include Enumerable
    def initialize; @all = {}; end
    def add(name)
      n = name.to_s
      n = "#{n} #{@all.length + 1}" if @all[n]
      m = (@all[n] = Material.new(n))
      SC.emit({ cmd: "material", name: n, color: SC.rgb(m.color) })
      m
    end
    def [](name); name.is_a?(Material) ? name : @all[name.to_s]; end
    def each(&b); @all.values.each(&b); end
    def length; @all.length; end
    def current; nil; end
  end

  class Layer
    attr_reader :name
    def initialize(name); @name = name; end
    def visible=(v); SC.emit({ cmd: "tag_visible", tag: @name, visible: v }); end
  end

  class Layers
    include Enumerable
    def initialize; @all = { "Layer0" => Layer.new("Layer0") }; end
    def add(name); @all[name.to_s] ||= Layer.new(name.to_s); end
    def [](name); @all[name.to_s]; end
    def each(&b); @all.values.each(&b); end
    def length; @all.length; end
  end

  # what was selected when the script ran: real SketchCraft ids
  class Selected < Group
    def initialize(id, name, kind); @ref_id = id; @name = name; @kind = kind; @entities = Entities.new(self); end
    def ref; @ref_id.to_s; end
    def jid; @ref_id; end
    def entityID; @ref_id; end
    def typename; @kind == "instance" ? "ComponentInstance" : @kind == "face" ? "Face" : "Group"; end
    # the same commands, by real id
    def material=(m); SC.paint([@ref_id], m); end
    def erase!; SC.emit({ cmd: "delete", ids: [@ref_id] }); @erased = true; end
    def transform!(t)
      t.ops.each do |op|
        case op[0]
        when :move then SC.emit({ cmd: "move", ids: [@ref_id], by: SC.m(op[1]) }, nil)
        when :rotate then SC.emit({ cmd: "rotate", ids: [@ref_id], center: SC.m(op[1]), axis: SC.vec(op[2]).normalize.to_a, angle: op[3] * 180.0 / Math::PI }, nil)
        else SC.emit({ cmd: "scale", ids: [@ref_id], center: SC.m(op[1]), factor: op[2] }, nil)
        end
      end
      self
    end
  end

  class Selection
    include Enumerable
    def initialize(items); @items = items; end
    def each(&b); @items.each(&b); end
    def to_a; @items; end
    def length; @items.length; end
    alias size length
    alias count length
    def empty?; @items.empty?; end
    def [](i); @items[i]; end
    def first; @items.first; end
    def clear; end
    def add(*_); end
  end

  class Model
    attr_reader :entities, :materials, :definitions, :layers
    def initialize
      @entities = Entities.new(nil)
      @materials = Materials.new
      @definitions = Definitions.new
      @layers = Layers.new
      @selection = Selection.new([])
      SC.seed_model(self)
    end
    def active_entities; @entities; end
    def selection; @selection; end
    def tags; @layers; end
    def start_operation(*_); true; end
    def commit_operation; true; end
    def abort_operation; SC.unsupported("Model#abort_operation (undo instead)"); end
    def title; "SketchCraft model"; end
    def path; ""; end
    def options; { "UnitsOptions" => { "LengthUnit" => 0 } }; end
    def shadow_info; {}; end
    def bounds; Geom::BoundingBox.new; end
    def method_missing(name, *_); SC.unsupported("Model##{name}"); end
    def respond_to_missing?(*_); false; end
  end

  @model = nil
  @loaded = {}

  def self.active_model; @model ||= Model.new; end
  def self.version; "24.0.0"; end
  def self.version_number; 24_000_000; end
  def self.is_pro?; true; end
  def self.platform; :platform_win; end
  def self.os_language; "en-US"; end
  def self.temp_dir; "/tmp"; end
  def self.find_support_file(name, *_); name; end
  def self.require(path); `Opal.require(#{path.to_s})`; true; end
  def self.register_extension(ext, load = true)
    `Opal.require(#{ext.path.to_s})` if load && ext.path
    true
  end
  def self.status_text=(t); end
  def self.set_status_text(*_); end
  def self.send_action(*_); true; end
  def self.method_missing(name, *_); SC.unsupported("Sketchup.#{name}"); end
end

class SketchupExtension
  attr_accessor :name, :path, :description, :version, :creator, :copyright
  def initialize(name, path); @name = name; @path = path; end
end

class LanguageHandler
  def initialize(*_); end
  def [](key); key; end
  def GetString(key); key; end
end

module UI
  @items = [] # [label, block]

  def self.items; @items; end
  def self.register(label, blk)
    @items.push([label.to_s, blk])
    `globalThis.__menu.push(#{label.to_s})`
  end

  class Menu
    def initialize(path); @path = path; end
    def add_item(item, &blk)
      if item.is_a?(UI::Command)
        UI.register(item.menu_text, item.proc)
      else
        UI.register(item.to_s, blk)
      end
      UI.items.length
    end
    def add_submenu(name); Menu.new(@path + [name.to_s]); end
    def add_separator; end
    def set_validation_proc(*_); end
  end

  class Command
    attr_accessor :tooltip, :status_bar_text, :small_icon, :large_icon, :menu_text
    attr_reader :proc
    def initialize(label, &blk); @menu_text = label.to_s; @proc = blk; end
    def set_validation_proc(*_); end
  end

  class Toolbar
    include Enumerable
    def initialize(name); @name = name; @cmds = []; end
    def add_item(cmd)
      @cmds.push(cmd)
      UI.register(cmd.menu_text, cmd.proc) unless UI.items.any? { |i| i[0] == cmd.menu_text }
    end
    def add_separator; end
    def show; end
    def restore; end
    def get_last_state; 1; end
    def each(&b); @cmds.each(&b); end
  end

  def self.menu(name = "Plugins"); Menu.new([name.to_s]); end
  def self.messagebox(msg, *_); SC.toast(msg); 1; end
  def self.beep; end
  def self.openURL(*_); true; end

  # inputbox(prompts, defaults, title) or (prompts, defaults, lists, title):
  # the first run asks the app for a form; the run is then repeated with the answers
  def self.inputbox(prompts, defaults = [], lists = nil, title = nil)
    if title.nil? && lists.is_a?(String)
      title = lists
      lists = nil
    end
    k = `globalThis.__askCount++`
    answers = `globalThis.__answers`
    have = `#{answers}.length > #{k}`
    unless have
      `globalThis.__need = {prompts: #{SC.js(prompts.map(&:to_s))}, defaults: #{SC.js(defaults.map { |d| d.nil? ? "" : d.to_s })}, lists: #{SC.js((lists || []).map { |l| l.to_s })}, title: #{(title || "Input").to_s}}`
      raise "__sketchcraft_needs_input__"
    end
    got = `#{answers}[#{k}]`
    prompts.each_with_index.map do |_, i|
      v = `#{got}[#{i}]`
      d = defaults[i]
      if d.is_a?(Integer) then v.to_s.to_i
      elsif d.is_a?(Float)
        s = v.to_s
        s =~ /[a-z'"]/i ? s.to_l : s.to_f
      else v.to_s
      end
    end
  end

end

MB_OK = 0
MB_OKCANCEL = 1
MB_YESNO = 4
IDOK = 1
IDYES = 6
IDNO = 7

module Kernel
  # SketchUp's sketchup.rb helpers
  def file_loaded?(_f); false; end
  def file_loaded(_f); end
  def add_separator_to_menu(_m); end
end

# what scripts print, collected for the app
class SCOut
  def write(*a); a.each { |s| `globalThis.__out.push(#{s.to_s})` }; nil; end
  def puts(*a); a.each { |s| write(s.to_s + "\n") }; nil; end
  def print(*a); write(*a); end
  def flush; self; end
  def sync=(_); end
end
$stdout = SCOut.new
$stderr = SCOut.new
