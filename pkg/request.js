export function http_request(x, y, tile_size) {
	var xhttp = new XMLHttpRequest();
	xhttp.onload = function(oEvent) {
		var arraybuffer = xhttp.response;
		if (arraybuffer) {
			var canvas = document.getElementById("framebuffer");
			var ctx = canvas.getContext('2d');

			var data = new Uint8ClampedArray(4 * tile_size * tile_size);
			var byteArray = new Uint8Array(arraybuffer);
			var index=0;
			for (var i = 0; i < tile_size; ++i) {
				for (var j = 0; j < tile_size; ++j) {
					var base = i * tile_size + j;
					data[4 * base] = byteArray[3*base];
					data[4 * base + 1] = byteArray[3*base+1];
					data[4 * base + 2] = byteArray[3*base+2];
					data[4 * base + 3] = 255;
				}
			}
			var imageData = new ImageData(data, tile_size, tile_size);
			var tempcanvas = document.createElement('canvas');
			tempcanvas.width = tile_size;
			tempcanvas.height = tile_size;
			var tempctx = tempcanvas.getContext('2d');
			tempctx.putImageData(imageData, 0, 0);

			//			ctx.scale(10,10);
			ctx.drawImage(tempcanvas, x*tile_size, y*tile_size);
		}
	};
	xhttp.open("POST", "https://pbrt-worker.edgecompute.app/rendertile", true);
	xhttp.responseType = "arraybuffer";

	var body = {};
	body["x"] = x;
	body["y"] = y;
	body["tile_size"] = tile_size;
	var body_str = JSON.stringify(body);

    xhttp.send(body_str);
}
