uniform struct Camera
{
    mat4 view;
    mat4 projection;
} camera;

uniform mat4 modelMatrix;

in vec3 position; //mesh vertex coordinates
out vec3 texPos;

void main()
{
    vec4 positionVec4 = vec4(position, 1.);

    vec4 vPosition = camera.projection * camera.view * modelMatrix * positionVec4;

    gl_Position = vPosition;

    texPos = (modelMatrix * positionVec4).xyz + vec3(0.5, 0.5, 0.5);
}
