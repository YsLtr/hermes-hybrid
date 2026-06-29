        sys.stderr.flush()

        while True:
            try:
                line = sys.stdin.readline()
                if not line:
                    sys.stderr.write("stdin closed, exiting\n")
                    break

                await self.handle_request(line.strip())

            except Exception as e:
                sys.stderr.write(f"Error in main loop: {e}\n")
                sys.stderr.flush()

    async def handle_request(self, line: str):
        """Parse and dispatch JSON-RPC request."""
        if not line:
            return
