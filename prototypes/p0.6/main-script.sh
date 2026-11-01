trap 'echo "WINCH size=$(stty size)"' WINCH
echo "main pid=$$ size=$(stty size)"
i=0
while :; do
  i=$((i+1))
  echo "tick $i"
  if read -r -t 1 line; then echo "got: $line"; fi
done
