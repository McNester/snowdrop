FROM node:22-bookworm-slim

WORKDIR /app
COPY demo/test-flight-speed-demo/package.json demo/test-flight-speed-demo/package-lock.json ./
RUN npm ci --omit=dev

COPY demo/test-flight-speed-demo/src ./src
COPY demo/test-flight-speed-demo/scripts ./scripts
CMD ["npm", "start"]
