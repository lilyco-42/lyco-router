#!/bin/sh
# lyco-tool —— 把任意 CLI 工具接成 lyco 服务（一条命令搞定全部）
#
# 用法:
#   lyco-tool add --name brush --title "brush shell" --cmd "brush -c"
#
# 做的事: 注册 manifest(自动分配端口) → 写通用 CLI 桥 → 写 systemd 单元 → 重启收编 → 自检
set -e
ROOT=/etc/lyco-router
BRIDGE=/opt/lyco/cli-bridge.py
PY=/usr/bin/python3

NAME=""; TITLE=""; CMD=""; KIND="custom"
while [ $# -gt 0 ]; do
  case "$1" in
    add) shift ;;
    --name)  NAME="$2";  shift 2 ;;
    --title) TITLE="$2"; shift 2 ;;
    --cmd)   CMD="$2";   shift 2 ;;
    --kind)  KIND="$2";  shift 2 ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done
[ -n "$NAME" ] || { echo "缺 --name" >&2; exit 1; }
[ -n "$CMD" ]  || { echo "缺 --cmd"  >&2; exit 1; }
[ -n "$TITLE" ] || TITLE="$NAME"
[ -f "$BRIDGE" ] || { echo "缺 $BRIDGE" >&2; exit 1; }

echo "== [1/4] 注册 manifest（端口自动分配）=="
OUT=$(lyco-router add --kind "$KIND" --name "$NAME" --title "$TITLE" --root "$ROOT" 2>&1) || { echo "$OUT"; exit 1; }
echo "$OUT" | grep -E '已注册|端口' || true
MANIFEST="$ROOT/services/$NAME/service.toml"
BACKEND=$(sed -n 's/^backend *= *"127\.0\.0\.1:\([0-9][0-9]*\)".*/\1/p' "$MANIFEST" | head -1)
LISTEN=$(sed -n 's/^listen *= *"0\.0\.0\.0:\([0-9][0-9]*\)".*/\1/p' "$MANIFEST" | head -1)
[ -n "$BACKEND" ] || { echo "拿不到 backend 端口（$MANIFEST）" >&2; exit 1; }
echo "   前端 $LISTEN → 后端 $BACKEND"

echo "== [2/4] 写 systemd 单元 =="
cat > "/etc/systemd/system/lyco-$NAME.service" <<UNIT
[Unit]
Description=$TITLE（lyco-router 按需拉起）
After=network.target

[Service]
Type=simple
ExecStart=$PY $BRIDGE $BACKEND $CMD
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
echo "   /etc/systemd/system/lyco-$NAME.service"

echo "== [3/4] 重启 lyco-router 收编 =="
systemctl restart lyco-router
sleep 2

echo "== [4/4] 自检 =="
systemctl start "lyco-$NAME" || true
sleep 2
printf '   服务: '; systemctl is-active "lyco-$NAME" || true
printf '   health: '; curl -s -m 5 "http://127.0.0.1:$BACKEND/health" || echo '(失败)'
echo
systemctl stop "lyco-$NAME" || true
echo "== 完成 =="
echo "   管理页: http://<板子IP>:8080/"
echo "   调用:   curl -X POST http://<板子IP>:$LISTEN/ -d '<参数>'"
