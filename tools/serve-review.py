# レビュー資料の受け渡し（内蔵ブラウザの ChatGPT・Tripo のページとファイルをやりとりする）。
# GET  /<名前>            .snaps/review の画面写真などを配る（ページから fetch して添付する）
# POST /inbox/<名前>      ページで作った物（生成画像・GLB）を .snaps/inbox/<名前> に保存する
# CORS と Private Network Access を許す。使い方: python3 tools/serve-review.py → http://127.0.0.1:4344/（ポート台帳の character-maker-review-share）
import http.server, os, re
root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '.snaps')
review = os.path.join(root, 'review'); inbox = os.path.join(root, 'inbox')
os.makedirs(review, exist_ok=True); os.makedirs(inbox, exist_ok=True)
class H(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k): super().__init__(*a, directory=review, **k)
    def end_headers(self):
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Private-Network', 'true')
        self.send_header('Cache-Control', 'no-store')
        super().end_headers()
    def do_OPTIONS(self):
        self.send_response(204); self.send_header('Access-Control-Allow-Headers', '*'); self.send_header('Access-Control-Allow-Methods', 'GET, POST'); self.end_headers()
    def do_POST(self):
        m = re.fullmatch(r'/inbox/([A-Za-z0-9._-]{1,120})', self.path)
        if not m: self.send_response(404); self.end_headers(); return
        n = int(self.headers.get('Content-Length', '0'))
        data = self.rfile.read(n)
        with open(os.path.join(inbox, m.group(1)), 'wb') as f: f.write(data)
        self.send_response(200); self.send_header('Content-Type', 'text/plain'); self.end_headers(); self.wfile.write(str(len(data)).encode())
http.server.ThreadingHTTPServer(('127.0.0.1', 4344), H).serve_forever()
