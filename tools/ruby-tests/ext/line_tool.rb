# A mouse tool, written the way extensions write them: click twice to draw an edge, type a length,
# Esc to start over. Uses Sketchup::Tool, input points, the status bar and the VCB.
module SCFixtures
  class LineTool
    def activate
      @points = []
      @ip = Sketchup::InputPoint.new
      Sketchup.status_text = 'Click the first point'
      Sketchup.vcb_label = 'Length'
    end

    def deactivate(view)
      view.invalidate
    end

    def onMouseMove(flags, x, y, view)
      @ip.pick(view, x, y)
      view.invalidate
    end

    def onLButtonDown(flags, x, y, view)
      @ip.pick(view, x, y)
      return unless @ip.valid?
      @points << @ip.position
      if @points.length == 2
        model = Sketchup.active_model
        model.start_operation('Line tool', true)
        model.active_entities.add_line(@points[0], @points[1])
        model.commit_operation
        @points = []
        Sketchup.status_text = 'Click the first point'
      else
        Sketchup.status_text = 'Click the second point'
      end
    end

    def onUserText(text, view)
      return if @points.empty?
      len = text.to_l
      dir = @points[0].vector_to(@ip.position)
      dir.length = len
      p2 = @points[0] + dir
      Sketchup.active_model.active_entities.add_line(@points[0], p2)
      @points = []
    end

    def onCancel(reason, view)
      @points = []
      Sketchup.status_text = 'Click the first point'
    end

    def draw(view)
      return if @points.empty?
      view.drawing_color = 'red'
      view.draw_line(@points[0], @ip.position) if @ip.valid?
    end

    def enableVCB?; true; end
  end
end

UI.menu('Plugins').add_item('Line tool') do
  Sketchup.active_model.select_tool(SCFixtures::LineTool.new)
end
